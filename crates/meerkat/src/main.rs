/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! meerkat-shell: the on-screen serval host for Mere's chrome.
//!
//! A winit window that runs the reused chrome ([`meerkat::chrome_view`] over a
//! [`meerkat::Chrome`] wrapping the graphshell `ToolbarState`) through serval and
//! presents via netrender — pelt-live-shaped. It reuses pelt-live's lib for the
//! cascade → layout → paint → `Scene` builder ([`scene_from_scripted_dom`]) and
//! the point→node hit-test ([`hit_test_node`]), so this file is the window +
//! present + input-dispatch harness, not a second engine.
//!
//! ## Two roots, one window
//!
//! The window composites **two authorities**: the chrome root (the reused toolbar
//! / omnibar, diffed by the runner) in a top band, and the content root — an
//! [`Orrery`], the graph's spatial presentation — filling the rest. The chrome
//! runs through serval into a `Scene`; the orrery produces its own composited
//! `Scene` from the graph + physics. Each is rasterized and composited at its
//! band, neither root seeing the other's tree. Input routes by region: the chrome
//! band hit-tests the chrome root, the content band drives the orrery (pan / zoom
//! / drag / select), and keyboard modifiers feed both.
//!
//! The orrery is the graph-rooted content surface (modular integration plan, S1).
//! Next (S2): navigating a location adds a node and projects its media as a tile,
//! so the omnibar drives the graph rather than a synthesized page.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eidetic_fjall::FjallStore;
use layout_dom_api::LayoutDom;
use forme::GraphMemberId;
use meerkat::command::Command;
use platen::Workbench;
use platen_view::{
    workbench_view, WorkbenchAction, WorkbenchLogic, WorkbenchScene, WorkbenchTreeView,
    WORKBENCH_SHEET,
};
use meerkat::{
    chrome_view, submit_omnibar, Chrome, ChromeLogic, ChromeView, ContextAction, ContextItem,
};
use netrender::external_texture::ExternalTexturePlacement;
use netrender::{ColorLoad, NetrenderOptions};
use orrery_host::{CameraView, NodeState, Orrery, PointerButton, WHEEL_PAN_SCALE};
use pelt_live::{fragments_from_scripted_dom, hit_test_node, scene_from_scripted_dom, TextCursor};
use serval_layout::ScrollOffsets;
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::{key_event_from_winit, modifiers_from_winit, SurfaceHost};
use session_runtime::{
    content_store, session_graph_store, settings_store, view_intent_store, CameraSnapshot,
    PersistedSettings, ViewIntent,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey as WinitNamedKey};
use winit::window::{Window, WindowId};
use xilem_serval::{Modifiers, PointerClick, ServalAppRunner};

mod card;
mod constellation;
mod content;
mod fetch;
mod resources;
mod sync;

use constellation::Constellation;

/// Author CSS for the **chrome** root. The toolbar is a flex row (back / forward
/// buttons + a growing omnibar); serval lays it out via taffy's flexbox. The
/// `.chrome` container has no background — the host composites it over the
/// content root, so only the toolbar and the (opaque) suggestions dropdown paint
/// over the page; everything else stays transparent.
// Dark-mode palette, coherent with the orrery's `surface_bg` (~rgb(17,20,26)):
// the toolbar band sits a step above it, fields/buttons a step above that, text
// near-white, accents blue-tinted. A light variant + a runtime toggle are the
// follow-up; this sheet is the chrome half of that seam.
const CHROME_SHEET: &[&str] = &[
    "div, button, input { display: block; }",
    ".toolbar { display: flex; background-color: rgb(28, 31, 38); padding: 8px; }",
    "button { font-size: 22px; color: rgb(222, 226, 234); \
        background-color: rgb(44, 48, 58); padding: 8px 14px; margin: 4px; }",
    ".disabled { color: rgb(108, 114, 126); background-color: rgb(34, 37, 45); }",
    "input { font-size: 22px; color: rgb(232, 234, 240); \
        background-color: rgb(36, 39, 48); padding: 8px; margin: 4px; flex-grow: 1; }",
    // The p2p sync chip: small + muted, no flex-grow, so the omnibar pushes it to
    // the toolbar's right edge.
    ".sync-chip { font-size: 14px; color: rgb(150, 156, 168); \
        background-color: rgb(38, 42, 52); padding: 8px 12px; margin: 4px; }",
    ".suggestions { background-color: rgb(30, 33, 41); padding-bottom: 6px; }",
    ".suggestion { font-size: 18px; color: rgb(206, 210, 220); \
        background-color: rgb(30, 33, 41); padding: 8px 16px; }",
    ".suggestion-active { font-size: 18px; color: rgb(234, 238, 246); \
        background-color: rgb(48, 58, 82); padding: 8px 16px; }",
    // Command palette: a centered panel floated over the page (flex centering;
    // serval maps justify-content through stylo_taffy).
    ".palette-overlay { display: flex; justify-content: center; padding-top: 56px; }",
    ".palette { width: 540px; background-color: rgb(34, 37, 46); padding: 10px; }",
    ".cmd-list { background-color: rgb(34, 37, 46); }",
    ".cmd-row { font-size: 18px; color: rgb(206, 210, 220); \
        background-color: rgb(34, 37, 46); padding: 8px 12px; }",
    ".cmd-row-active { font-size: 18px; color: rgb(234, 238, 246); \
        background-color: rgb(46, 56, 80); padding: 8px 12px; }",
    // Settings overlay: a centered panel (like the palette) with rows of controls.
    ".settings-overlay { display: flex; justify-content: center; padding-top: 56px; }",
    ".settings { width: 380px; background-color: rgb(34, 37, 46); padding: 14px; }",
    ".set-title { display: flex; background-color: rgb(34, 37, 46); padding: 4px 4px 12px 4px; }",
    ".set-title-text { font-size: 20px; color: rgb(234, 238, 246); \
        background-color: rgb(34, 37, 46); flex-grow: 1; padding: 4px 8px; }",
    ".set-row { display: flex; background-color: rgb(34, 37, 46); padding: 6px 8px; }",
    ".set-value { font-size: 18px; color: rgb(206, 210, 220); \
        background-color: rgb(34, 37, 46); padding: 8px 14px; flex-grow: 1; }",
    ".set-btn { font-size: 20px; color: rgb(222, 226, 234); \
        background-color: rgb(48, 52, 62); padding: 6px 16px; margin: 0 4px; }",
    // Right-click context menu: a small panel of action rows floated at the cursor.
    ".context-menu { background-color: rgb(38, 42, 52); padding: 4px; }",
    ".context-item { font-size: 16px; color: rgb(216, 220, 230); \
        background-color: rgb(38, 42, 52); padding: 8px 18px; }",
];

/// Fallback chrome-band height (px) if the toolbar can't be measured.
const FALLBACK_TOOLBAR_H: u32 = 64;

/// Background of the floating content card — a panel a step above the orrery
/// backdrop, so the card reads as a raised surface over the dark orrery band.
const CARD_BG: wgpu::Color = wgpu::Color { r: 0.110, g: 0.122, b: 0.145, a: 1.0 };

/// Single-pane view-intent identity for the default session (one frame, one
/// pane). Per-frame / per-pane ids arrive with the tiled workbench (S4) and
/// session manifests (S3.2b).
const DEFAULT_FRAME: &str = "00000000-0000-0000-0000-0000000f1a3e";
const DEFAULT_PANE: u64 = 0;

/// The meerkat shell application: the shared chrome DOM, the runner that diffs
/// the chrome view tree into it, the orrery content-root, the window + GPU, and
/// input bookkeeping.
struct App {
    /// The chrome DOM the runner mutates and the render path reads.
    dom: Rc<RefCell<ScriptedDom>>,
    runner: ServalAppRunner<Chrome, ChromeLogic, ChromeView>,
    /// The content root: the [`Orrery`] — the graph's spatial presentation,
    /// rendered into the band below the chrome and driven by content-band input.
    orrery: Orrery,
    /// The navigation target last synced into the orrery via `visit`; guards
    /// re-visiting. Mirrors the chrome's `content_location`.
    content_location: String,
    /// Whether the orrery has been centered on its content band yet (done once,
    /// the first render after the toolbar height is known).
    centered: bool,
    /// The fetch actor's command handle (the kernel commands it over this; its
    /// outcomes arrive on `inbox.fetch`).
    fetch_handle: armillary::ActorHandle<fetch::FetchCommand>,
    /// The constellation: the pool of active nodes (their content actors). The
    /// focused card (Cartography) and the workbench tiles (Tree) both draw their
    /// scenes from here — one activation lifecycle, not two. Reconciled to the
    /// needed set each frame; backgrounded nodes outlive the view.
    constellation: Constellation,
    /// The p2p sync subsystem (S5.0 / S5.1): owns the transport + the tessera lane
    /// on its own runtime. Status changes arrive on `inbox.sync` and fold into the
    /// chrome sync chip; the "connect to peer" verb drives it via `sync.connect`.
    sync: sync::SyncHost,
    /// Per-URL fetched content state, keyed by the node's URL (URL identity).
    content: HashMap<String, fetch::ContentState>,
    /// The session's data directory (`<data_dir>/mere`): holds `graph.json` and
    /// the `views/` view-intent sidecars.
    session_dir: PathBuf,
    /// Durable content cache (S3.2c) under the session dir, persisting fetched
    /// pages + subresources by URL. `None` if the store could not be opened
    /// (caching disabled; the shell still runs).
    store: Option<FjallStore>,
    /// Cached measured height (px) of the chrome band; `0` until first measured.
    toolbar_h: u32,
    window: Option<Arc<Window>>,
    /// The shared serval-on-winit present stack, built once a window exists.
    host: Option<SurfaceHost>,
    /// Tracked keyboard modifiers, folded into each dispatched `KeyEvent`.
    modifiers: Modifiers,
    /// Last cursor position in physical pixels (window space == content space).
    cursor: (f32, f32),
    /// The last left-button release (time + window pos), for double-click detection.
    /// A double-click on an orrery node opens the tiled workbench from it.
    last_left_release: Option<(Instant, (f32, f32))>,
    /// The tile in focus in the tiled view (the last activated / opened member), so
    /// the omnibar can show its URL. `None` outside the tiled view or with no tiles.
    focused_tile: Option<GraphMemberId>,
    /// The location last pushed into the omnibar by focus-follow, so it only updates
    /// when the focused tile / node actually changes (not every frame).
    shown_location: Option<String>,
    /// An in-progress tab drag in the tiled view: the pressed tab's member + the
    /// press position. Resolved on release — a move when dragged past the slop, else
    /// it was a plain click (the tab already activated on press).
    tab_drag: Option<(GraphMemberId, (f32, f32))>,
    /// Each open tile's content rect in window coords (member, [x0, y0, x1, y1]),
    /// recomputed each tiled frame. The drag uses it to resolve the drop target
    /// under the pointer + which zone (center = move/stack, edge = split).
    tile_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Cached rasterized texture per tile, keyed by member. Re-rasterized only when
    /// the tile's scene version or size changes, so an unchanged tile is composited
    /// from its cached texture instead of re-rasterized every frame (the cost that
    /// scaled with tile count). Evicted when a tile closes.
    tile_textures: HashMap<GraphMemberId, CachedTile>,
    /// An in-progress divider drag: the left-slot index, the press x, and the slot
    /// weights snapshot at press. Cursor moves reweight the two neighbouring slots.
    divider_drag: Option<(usize, f32, Vec<f32>)>,
    width: u32,
    height: u32,
    /// The tiled-workbench composition (S4): the open tiles + the projection mode
    /// (Cartography = the orrery, Tree = the tiled view).
    workbench: Workbench,
    /// The workbench root: a second serval document authority (separate from the
    /// chrome root) that renders the tile tree as flex DOM via [`platen_view`].
    /// In Tree mode the host syncs it from `workbench`, rasterizes it as the content
    /// band, reads each tile's content rect back, and composites that tile's actor
    /// texture there. taffy lays it out — the morphorm path is gone.
    workbench_dom: Rc<RefCell<ScriptedDom>>,
    workbench_runner: ServalAppRunner<WorkbenchScene, WorkbenchLogic, WorkbenchTreeView>,
    /// The members the open right-click context menu acts on (the selection's
    /// working set, captured when the menu opened). Empty when no menu is open.
    context_set: Vec<GraphMemberId>,
    /// The active-tab cap last written to the settings sidecar. Guards the persist
    /// path so an unchanged value isn't re-written on every chrome click.
    saved_tab_cap: usize,
    /// The kernel inbox: the typed receivers the I/O actors deliver on, behind the
    /// one winit wake. `user_event` is the single documented place that reads them.
    inbox: KernelInbox,
    /// Marks this struct as the kernel-thread context: `!Send` by construction
    /// (armillary's typed boundary), so kernel authority cannot be moved onto an
    /// actor thread — the attempt is a compile error, not a review catch.
    _kernel: armillary::KernelThread,
}

/// A tile's cached rasterized texture: the scene version + size it was rasterized
/// at, plus the GPU texture and its view. Reused across frames while the version +
/// size hold, so an idle tile is not re-rasterized.
struct CachedTile {
    version: u64,
    size: (u32, u32),
    #[allow(dead_code)] // owns the texture the `view` references; kept alive here
    tex: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The host kernel's inbox: the typed receivers each I/O actor delivers updates on,
/// all woken by the one bare `EventLoopProxy<()>`. Grouping them names the seam (the
/// kernel/actor boundary's inbound half) without collapsing the per-subsystem
/// channels into one mega enum, which would muddy ownership. The constellation's
/// per-tile content channels are drained separately (`Constellation::drain`); this
/// holds only the I/O streams.
struct KernelInbox {
    fetch: Receiver<fetch::FetchUpdate>,
    sync: Receiver<sync::SyncUpdate>,
}

impl App {
    fn new(proxy: EventLoopProxy<()>) -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        // The workbench root's own document, separate from the chrome root (the
        // separate-roots discipline). Empty until the tiled view syncs it.
        let workbench_dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        // The session lives under the per-user data dir; restore the graph + the
        // camera (view-intent) + the settings on launch, else seed fresh.
        let session_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("mere");
        let _ = std::fs::create_dir_all(&session_dir);
        // Restore persisted settings (the active-tab cap) so the chrome + actor pool
        // open at the user's saved value rather than the default.
        let saved_settings =
            settings_store::load_settings(&session_dir).ok().flatten().unwrap_or_default();
        let mut chrome = Chrome::new("mere://welcome");
        chrome.settings.tab_cap = saved_settings.tab_cap;
        let runner = ServalAppRunner::new(dom.clone(), chrome_view as ChromeLogic, chrome);
        let content_location = runner.state().content_location().to_string();
        // Durable content cache (S3.2c) in a fjall keyspace under the session dir;
        // `None` simply disables caching (the shell runs without it).
        let store = match FjallStore::open(session_dir.join("content")) {
            Ok(store) => Some(store),
            Err(err) => {
                tracing::warn!(%err, "content cache unavailable; running without it");
                None
            },
        };
        let graph_file = session_dir.join(session_graph_store::GRAPH_FILE);
        let restored = match session_graph_store::load(&graph_file) {
            Ok(Some(graph)) => {
                tracing::info!(path = ?graph_file, "restored the session graph");
                Some(graph)
            },
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(%err, path = ?graph_file, "session graph load failed; starting fresh");
                None
            },
        };
        let mut orrery = match restored {
            Some(graph) => Orrery::with_graph(graph),
            None => {
                // The orrery opens on one node and grows from there as the user
                // navigates (the graph-rooted browse loop).
                let mut orrery = Orrery::new();
                if !content_location.is_empty() {
                    orrery.visit(&content_location);
                }
                orrery
            },
        };
        // Restore the view-intent (camera + focused node) so the spatial view and
        // the open card persist across restarts. A restored camera suppresses the
        // first-frame recenter; the focused node re-selects (if it still exists).
        let restored_view =
            view_intent_store::load_view_intent(&session_dir, DEFAULT_FRAME, DEFAULT_PANE)
                .ok()
                .flatten();
        let restored_camera = restored_view.as_ref().and_then(|v| v.camera);
        if let Some(snapshot) = &restored_camera {
            orrery.set_camera(snapshot_to_camera(snapshot));
        }
        if let Some(url) = restored_view.as_ref().and_then(|v| v.focus.as_deref()) {
            orrery.select_by_url(url);
        }
        // The fetch actor wakes the loop through the winit proxy; armillary takes
        // the wake as a host-neutral callback.
        let fetch_proxy = proxy.clone();
        let fetch_wake: armillary::Wake = Arc::new(move || {
            let _ = fetch_proxy.send_event(());
        });
        let (fetch_handle, fetch_rx) = fetch::spawn_fetcher(fetch_wake);
        // The content actor renders the focused card off the UI thread (it owns the
        // serval cascade + nematic engines + a per-tile subresource cache on its own
        // thread) and ships scenes / wanted subresources / harvested linked data
        // back through the same wake.
        let content_proxy = proxy.clone();
        let content_wake: armillary::Wake = Arc::new(move || {
            let _ = content_proxy.send_event(());
        });
        let mut constellation = Constellation::new(content_wake);
        constellation.set_cap(saved_settings.tab_cap);
        // The p2p sync subsystem: binds the transport + joins the tessera demo
        // moot on its own runtime, delivering status changes through `proxy` (the
        // same wake the fetch actor uses). Setup failure disables p2p, not the shell.
        let (sync, sync_rx) = sync::SyncHost::new(proxy.clone(), sync::DEMO_MOOT);
        Self {
            dom,
            runner,
            orrery,
            content_location,
            centered: restored_camera.is_some(),
            fetch_handle,
            constellation,
            sync,
            content: HashMap::new(),
            session_dir,
            store,
            toolbar_h: 0,
            window: None,
            host: None,
            modifiers: Modifiers::default(),
            cursor: (0.0, 0.0),
            last_left_release: None,
            focused_tile: None,
            shown_location: None,
            tab_drag: None,
            tile_rects: Vec::new(),
            tile_textures: HashMap::new(),
            divider_drag: None,
            width: 1024,
            height: 600,
            workbench: Workbench::new(),
            workbench_dom: workbench_dom.clone(),
            workbench_runner: ServalAppRunner::new(
                workbench_dom,
                workbench_view as WorkbenchLogic,
                WorkbenchScene::default(),
            ),
            context_set: Vec::new(),
            saved_tab_cap: saved_settings.tab_cap,
            inbox: KernelInbox { fetch: fetch_rx, sync: sync_rx },
            _kernel: armillary::KernelThread::new(),
        }
    }

    /// The toolbar-band height (px), measuring + caching it on first use. The
    /// toolbar is a single flex row, so its border-box height is independent of
    /// the available width/height; measuring once suffices. Used to place the
    /// content root directly below the toolbar.
    fn toolbar_height(&mut self) -> u32 {
        if self.toolbar_h == 0 {
            self.toolbar_h = measure_class_bottom(&self.dom.borrow(), self.width, self.height, "toolbar")
                .unwrap_or(FALLBACK_TOOLBAR_H);
        }
        self.toolbar_h
    }

    /// Reconfigure the surface for `(width, height)` and request a redraw.
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        if let Some(host) = self.host.as_mut() {
            host.resize(self.width, self.height);
        }
        self.request_redraw();
    }

    /// Render the two authorities and present them. The orrery content root fills
    /// everything below the toolbar; the chrome root is rendered over the full
    /// window with a *transparent* clear, so its toolbar band and any open
    /// dropdown float above the content while the rest lets the orrery show
    /// through. Composite order is content first, then chrome on top.
    fn render(&mut self) {
        if self.host.is_none() {
            return;
        }
        let (w, h) = (self.width.max(1), self.height.max(1));
        let toolbar_h = self.toolbar_height().min(h);
        let content_h = h.saturating_sub(toolbar_h).max(1);

        // Chrome scene over the full window. Paint the caret / selection of the
        // focused field — the palette query when open, else the omnibar (byte
        // offsets from the field's char model).
        let cursor = self.runner.focus().map(|node| {
            let field = self.runner.state().active_field();
            let byte_of = |i: usize| {
                field.text().char_indices().nth(i).map(|(b, _)| b).unwrap_or(field.text().len())
            };
            let selection = field.has_selection().then(|| {
                let (s, e) = field.selection();
                (byte_of(s), byte_of(e))
            });
            TextCursor { node, caret: field.caret_byte_in_render(), selection }
        });
        let scroll = ScrollOffsets::<NodeId>::default();
        let chrome_scene =
            scene_from_scripted_dom(&self.dom.borrow(), CHROME_SHEET, w, h, cursor, &scroll);

        // Color the orrery's nodes by activation state (green open / red closed /
        // blue new) so the graph shows at a glance what's live. (Visible in
        // Cartography; the orrery is hidden in the tiled view.)
        let states = self.node_states();
        self.orrery.set_node_states(states);

        // The content root. In Cartography the orrery composites its own scene over
        // the band (kept in sync, centered once). In the tiled workbench the orrery
        // is hidden behind the tiles, so skip its physics + paint entirely and back
        // the band with an empty dark-cleared scene — the tiles composite over it,
        // the splitter gutters show the dark. Skipping it also stops the orrery's
        // settle / glide redraw loop, which would otherwise re-rasterize every tile
        // each frame behind the cover.
        let (content_scene, orrery_redraw) = if self.workbench.is_tiled() {
            // Tree: the workbench root (a serval flex-DOM document) is the band. Sync
            // it from the model + graph + pin state, then rasterize it — taffy lays
            // the tiles out (no morphorm). The orrery is hidden, so its physics +
            // paint are skipped.
            let mut scene = WorkbenchScene::from_workbench(
                &self.workbench,
                self.orrery.graph(),
                (w as f32, content_h as f32),
                |m| self.constellation.is_background(m),
                |m| self.constellation.is_recovering(m),
            );
            // Highlight the slot under the pointer while a tab is being dragged
            // (uses last frame's tile rects; the slots don't move, so the lag is
            // imperceptible).
            scene.drag_target = self.drag_target_member();
            if self.workbench_runner.state() != &scene {
                self.workbench_runner.update(move |s| *s = scene);
            }
            let wb = scene_from_scripted_dom(
                &self.workbench_dom.borrow(),
                WORKBENCH_SHEET,
                w,
                content_h,
                None,
                &scroll,
            );
            (wb, false)
        } else {
            self.orrery.resize(w, content_h);
            if !self.centered {
                self.orrery.recenter();
                self.centered = true;
            }
            self.orrery.frame(w, content_h)
        };

        // Reconcile the active-node pool to what this frame shows — the open tiles
        // (Tree) or the focused node (Cartography). Needed-but-dormant nodes spawn
        // an actor; active nodes no longer shown are reaped, unless backgrounded.
        let needed = self.needed_members();
        self.constellation.reconcile(&needed);

        // Content cards floating over the band: one per laid-out tile in Tree, the
        // focused-node card at `card_rect` in Cartography. Each entry is
        // `(member, window dest rect, raster size)`; the scene comes from that
        // node's activation at composite time. Driving an activation re-renders it
        // off the UI thread only when its document or size changed.
        let mut cards: Vec<(GraphMemberId, [f32; 4], (u32, u32))> = Vec::new();
        if self.workbench.is_tiled() {
            // Read each content placeholder's laid-out rect + member out of the
            // workbench DOM (taffy laid it out above), then drive that tile's actor
            // and queue it to composite at that rect (window coords add `toolbar_h`).
            // taffy layouts are *parent-relative*, so sum the workbench > slot >
            // content chain for an absolute rect — otherwise every slot's content
            // reports the same slot-local origin and the tiles stack on each other.
            // The collect releases the DOM borrow before we mutate self.
            // (member, content rect, full slot rect) in window coords. The content
            // rect is where the tile's texture composites (below the strip); the slot
            // rect is the whole column (strip + content), used as the drag target so
            // dragging along the strip still resolves + highlights its slot.
            let placements: Vec<(GraphMemberId, [f32; 4], [f32; 4])> = {
                let th = toolbar_h as f32;
                let dom = self.workbench_dom.borrow();
                let frags = fragments_from_scripted_dom(&dom, WORKBENCH_SHEET, w, content_h);
                let root = dom.document();
                let (wx, wy) = first_with_class(&dom, root, "workbench")
                    .and_then(|n| frags.rect_of(n))
                    .map(|l| (l.location.x, l.location.y))
                    .unwrap_or((0.0, 0.0));
                all_with_class(&dom, root, "wb-slot")
                    .into_iter()
                    .filter_map(|slot| {
                        let sl = frags.rect_of(slot)?;
                        let content = first_with_class(&dom, slot, "wb-content")?;
                        let member = member_attr(&dom, content)?;
                        let cl = frags.rect_of(content)?;
                        let cx = wx + sl.location.x + cl.location.x;
                        let cy = th + wy + sl.location.y + cl.location.y;
                        let content_rect = [cx, cy, cx + cl.size.width, cy + cl.size.height];
                        let sx = wx + sl.location.x;
                        let sy = th + wy + sl.location.y;
                        let slot_rect = [sx, sy, sx + sl.size.width, sy + sl.size.height];
                        Some((member, content_rect, slot_rect))
                    })
                    .collect()
            };
            let mut slot_rects = Vec::with_capacity(placements.len());
            for (member, content, slot) in placements {
                slot_rects.push((member, slot));
                let Some(url) =
                    self.orrery.graph().get_node_by_id(member).map(|(_, n)| n.url().to_string())
                else {
                    continue;
                };
                self.ensure_content(&url);
                let cw = (content[2] - content[0]).round().max(1.0) as u32;
                let ch = (content[3] - content[1]).round().max(1.0) as u32;
                let state = self.content.get(&url).cloned();
                self.constellation.drive(member, &url, state, cw, ch);
                cards.push((member, content, (cw, ch)));
            }
            self.tile_rects = slot_rects;
        } else if let (Some(member), Some(url)) =
            (self.focused_member(), self.orrery.focused_url().map(str::to_string))
        {
            if let Some((x0, y0, x1, y1, cw, ch)) = card::card_rect(w, toolbar_h, h) {
                self.ensure_content(&url);
                let state = self.content.get(&url).cloned();
                self.constellation.drive(member, &url, state, cw, ch);
                cards.push((member, [x0, y0, x1, y1], (cw, ch)));
            }
            self.tile_rects.clear(); // no drag targets outside the tiled view
        } else {
            self.tile_rects.clear();
        }

        // The omnibar follows focus: point it at the focused tile / node when that
        // changed (next frame, like the chrome strips were — the scene above is
        // already built).
        self.sync_location();

        let host = self.host.as_ref().unwrap();
        let (_chrome_tex, chrome_view) =
            host.rasterize(&chrome_scene, w, h, ColorLoad::Clear(wgpu::Color::TRANSPARENT));
        // The orrery paints its own opaque backdrop, but clear to the same dark
        // tone so a resize frame cannot flash white before the backdrop lands.
        let (_content_tex, content_view) = host.rasterize(
            &content_scene,
            w,
            content_h,
            ColorLoad::Clear(wgpu::Color { r: 0.067, g: 0.078, b: 0.100, a: 1.0 }),
        );
        // Rasterize each tile's scene to an offscreen texture only when its version
        // or size changed; reuse the cached texture otherwise, so an unchanged tile
        // is not re-rasterized every frame (the cost that scaled with tile count).
        // The cache (self.tile_textures) keeps the textures alive across frames; evict
        // closed tiles first so theirs free. `composite` is what to draw, in order.
        self.tile_textures.retain(|m, _| cards.iter().any(|(cm, _, _)| cm == m));
        let mut composite: Vec<([f32; 4], GraphMemberId)> = Vec::with_capacity(cards.len());
        for (member, dest, (cw, ch)) in &cards {
            let version = self.constellation.scene_version(*member);
            let fresh = self
                .tile_textures
                .get(member)
                .is_some_and(|c| c.version == version && c.size == (*cw, *ch));
            if !fresh {
                if let Some(scene) = self.constellation.scene(*member) {
                    let (tex, view) = host.rasterize(scene, *cw, *ch, ColorLoad::Clear(CARD_BG));
                    self.tile_textures
                        .insert(*member, CachedTile { version, size: (*cw, *ch), tex, view });
                }
            }
            if self.tile_textures.contains_key(member) {
                composite.push((*dest, *member));
            }
        }

        let Some(frame) = host.acquire() else { return };
        let target_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let format = host.format();
        // Content fills [toolbar_h, h] (dest_rect is [x0, y0, x1, y1] corners;
        // viewport is the full surface). Then each content card floats over it, and
        // the transparent-cleared chrome composites over the whole window — toolbar
        // + dropdown on top, the rest letting the content through.
        host.renderer().compose_external_texture(
            &content_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, toolbar_h as f32, w as f32, h as f32]),
        );
        for (dest, member) in &composite {
            let Some(cached) = self.tile_textures.get(member) else { continue };
            host.renderer().compose_external_texture(
                &cached.view,
                &target_view,
                format,
                w,
                h,
                ExternalTexturePlacement::new(*dest),
            );
        }
        host.renderer().compose_external_texture(
            &chrome_view,
            &target_view,
            format,
            w,
            h,
            ExternalTexturePlacement::new([0.0, 0.0, w as f32, h as f32]),
        );
        frame.present();

        // Keep animating while the orrery is settling / gliding / dragging.
        if orrery_redraw {
            self.request_redraw();
        }
    }

    /// Request a redraw if a window exists.
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// The URL of whatever is in focus: in the tiled view the focused tile's node,
    /// in the orrery the focused node. `None` when nothing is focused.
    fn current_focus_url(&self) -> Option<String> {
        if self.workbench.is_tiled() {
            let member = self.focused_tile?;
            self.orrery.graph().get_node_by_id(member).map(|(_, node)| node.url().to_string())
        } else {
            self.orrery.focused_url().map(str::to_string)
        }
    }

    /// Point the omnibar at the focused tile / node (the address bar follows focus),
    /// but only when that focus actually changed and the user isn't editing the
    /// omnibar (no chrome field holds the caret) — so it never clobbers typing.
    fn sync_location(&mut self) {
        let url = self.current_focus_url();
        if url == self.shown_location {
            return;
        }
        self.shown_location = url.clone();
        if let (Some(url), None) = (url, self.runner.focus()) {
            self.runner.update(move |c| c.show_location(&url));
            self.request_redraw();
        }
    }

    /// Sync the orrery to the chrome's current navigation target: when the
    /// location changes, `visit` it — adding a node and a browse-trail edge, or
    /// selecting the existing node (URL identity). Called after any input that can
    /// navigate (omnibar submit, suggestion / back / forward clicks, palette).
    fn sync_orrery(&mut self) {
        let loc = self.runner.state().content_location().to_string();
        if loc != self.content_location {
            self.orrery.visit(&loc);
            self.ensure_content(&loc);
            self.content_location = loc;
            self.save_session();
            self.request_redraw();
        }
    }

    /// Persist the session (graph + camera view-intent) under the session dir.
    /// Best-effort: a write failure is logged, not fatal. Called after each
    /// navigation and on window close.
    fn save_session(&self) {
        let graph_file = self.session_dir.join(session_graph_store::GRAPH_FILE);
        if let Err(err) = session_graph_store::save(&graph_file, self.orrery.graph()) {
            tracing::warn!(%err, path = ?graph_file, "failed to persist the session graph");
        }
        let intent = ViewIntent {
            camera: Some(camera_to_snapshot(self.orrery.camera())),
            focus: self.orrery.focused_url().map(str::to_string),
            ..Default::default()
        };
        if let Err(err) =
            view_intent_store::save_view_intent(&self.session_dir, DEFAULT_FRAME, DEFAULT_PANE, &intent)
        {
            tracing::warn!(%err, dir = ?self.session_dir, "failed to persist the view intent");
        }
    }

    /// Make the focused node's content available. A network address already in
    /// this session's content map is left as-is; otherwise a durable cache hit is
    /// shown without re-fetching (so a reload need not hit the network), and a
    /// miss marks it `Loading` and spawns a fetch.
    fn ensure_content(&mut self, url: &str) {
        if !fetch::is_fetchable(url) || self.content.contains_key(url) {
            return;
        }
        if let Some(stored) = self.load_cached(url) {
            self.content.insert(url.to_string(), fetch::ContentState::Ready(fetched_from(stored)));
            return;
        }
        self.content.insert(url.to_string(), fetch::ContentState::Loading);
        self.fetch_handle.command(fetch::FetchCommand::Page(url.to_string()));
    }

    /// Toggle between the orrery (Cartography) and the tiled workbench (Tree).
    /// Entering Tree seeds the open set from the focused node and its graph
    /// neighbors, so the tiled view reflects the node you toggled on; exiting
    /// clears it. The constellation reconciles its actors to the resulting needed
    /// set on the next frame — spawning the tiles, reaping what's no longer shown
    /// (background-flagged nodes excepted).
    fn toggle_workbench(&mut self) {
        // Clear the omnibar suggestions dropdown so it doesn't hang over the tiles.
        self.runner.update(Chrome::close_suggestions);
        self.workbench.toggle_mode();
        self.workbench.clear_tiles();
        if self.workbench.is_tiled() {
            for member in self.selection_working_set() {
                self.workbench.open_tile(member);
            }
            // Focus the node the open was seeded from (the primary selection), so the
            // omnibar shows its URL; fall back to the first opened tile.
            self.focused_tile = self
                .orrery
                .selected_members()
                .first()
                .copied()
                .or_else(|| self.workbench.open_members().first().copied());
        }
        self.request_redraw();
    }

    /// The members a selection-driven open acts on. A multi-selection is its own
    /// nodes (opened in splits). A single selection expands to the **active tabs in
    /// that node's graphlet** — its connected component intersected with the warm-tab
    /// set, plus the node itself — so you gather the live cluster around it. An empty
    /// selection yields nothing. Shared by entering the workbench and the right-click
    /// menu.
    fn selection_working_set(&self) -> Vec<GraphMemberId> {
        let selected = self.orrery.selected_members();
        if selected.len() > 1 {
            return selected; // multi-select → the selection
        }
        match selected.first() {
            Some(&focus) => self
                .orrery
                .connected_members(focus)
                .into_iter()
                .filter(|m| *m == focus || self.constellation.is_active(*m))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Open the right-click context menu over the current selection's working set,
    /// at window `(x, y)`. A no-op when nothing is selected (no set to act on). A
    /// single-member set offers one "open tile"; a larger set offers splits vs a
    /// stack. The host remembers the set; the chrome renders the rows.
    fn open_context_menu_at(&mut self, x: f32, y: f32) {
        let set = self.selection_working_set();
        if set.is_empty() {
            return;
        }
        let items = if set.len() == 1 {
            vec![ContextItem::new("Open tile", ContextAction::OpenSplits)]
        } else {
            vec![
                ContextItem::new("Open in splits", ContextAction::OpenSplits),
                ContextItem::new("Open in a stack", ContextAction::Stack),
            ]
        };
        self.context_set = set;
        self.runner.update(move |c| c.open_context_menu(x, y, items));
        self.request_redraw();
    }

    /// Dismiss the context menu (an outside click / Escape), dropping its set.
    fn close_context_menu(&mut self) {
        self.context_set.clear();
        self.runner.update(Chrome::close_context_menu);
        self.request_redraw();
    }

    /// Run a pending context-menu action the chrome captured: open the menu's
    /// member set as splits or as one stack, switching into the tiled (Tree)
    /// projection first if needed.
    fn drain_pending_context(&mut self) {
        let Some(action) = self.runner.state().pending_context else {
            return;
        };
        self.runner.update(|c| c.pending_context = None);
        let set = std::mem::take(&mut self.context_set);
        if set.is_empty() {
            return;
        }
        // These open tiles, so surface the tiled view (closing the suggestions
        // dropdown on the way in, like Ctrl+T does).
        if self.workbench.ensure_tiled() {
            self.runner.update(Chrome::close_suggestions);
        }
        match action {
            ContextAction::OpenSplits => {
                self.workbench.open_split(&set);
            },
            ContextAction::Stack => {
                self.workbench.open_stack(&set);
            },
        }
        self.request_redraw();
    }

    /// Delete the focused node from the graph and reap its activation (the actor
    /// winds down on drop). A no-op when zero or many nodes are focused. Deletion
    /// removes the node's data; deactivation just stops its actor — this does
    /// both, because the node itself is gone.
    fn delete_focused_node(&mut self) {
        if let Some(member) = self.orrery.remove_focused() {
            self.constellation.reap(member);
            self.save_session();
            self.request_redraw();
        }
    }

    /// Toggle the focused node's background flag: when set, its actor keeps
    /// running after focus moves away (the headless-active state for background
    /// work). A no-op when nothing is focused or the focused node has no live
    /// actor yet (focused but not rendered — press again once it has).
    fn toggle_focus_background(&mut self) {
        let Some(member) = self.focused_member() else {
            return;
        };
        let next = !self.constellation.is_background(member);
        if self.constellation.set_background(member, next) {
            tracing::info!(%member, background = next, "toggled node background");
            self.request_redraw();
        }
    }

    /// The set of graph members that should be active this frame: in Tree the open
    /// tiles, in Cartography just the focused node (if any). The constellation
    /// reconciles its actor pool to this.
    fn needed_members(&self) -> Vec<GraphMemberId> {
        if self.workbench.is_tiled() {
            // Every tab across every slot stays warm, not just the visible ones, so
            // switching a stack's tab is instant (the actor already has its scene).
            self.workbench.open_members()
        } else {
            self.focused_member().into_iter().collect()
        }
    }

    /// The per-node activation state for the orrery's node coloring. A node with
    /// real fetched (`Ready`) content is `Open` (green) when a live actor is
    /// showing it, else `Closed` (red); everything else — a local / synthesized
    /// page, a blank (loading) one, or an errored one — is `Idle` (blue).
    fn node_states(&self) -> HashMap<GraphMemberId, NodeState> {
        self.orrery
            .graph()
            .nodes()
            .map(|(_key, node)| {
                let state = match self.content.get(node.url()) {
                    Some(fetch::ContentState::Ready(_)) => {
                        if self.constellation.is_active(node.id) {
                            NodeState::Open
                        } else {
                            NodeState::Closed
                        }
                    },
                    _ => NodeState::Idle,
                };
                (node.id, state)
            })
            .collect()
    }

    /// The focused node's graph member, if a node is focused (resolved URL → node
    /// UUID via the kernel node id).
    fn focused_member(&self) -> Option<GraphMemberId> {
        let url = self.orrery.focused_url()?;
        self.orrery.graph().get_node_by_url(url).map(|(_, node)| node.id)
    }

    /// Load durably-cached content for `url` (page or subresource), or `None`.
    /// The fjall store's futures are ready, so `block_on` does not stall the UI.
    fn load_cached(&mut self, url: &str) -> Option<content_store::StoredContent> {
        let store = self.store.as_mut()?;
        pollster::block_on(content_store::load_content(store, url)).ok().flatten()
    }

    /// Persist `body` (+ its content-type) for `url` to the durable content cache,
    /// so a reload need not re-fetch it. Best-effort; a write failure is logged.
    fn save_cached(&mut self, url: &str, content_type: Option<String>, body: &[u8]) {
        let Some(store) = self.store.as_mut() else {
            return;
        };
        let stored = content_store::StoredContent { content_type, body: body.to_vec() };
        if let Err(err) = pollster::block_on(content_store::save_content(store, url, &stored)) {
            tracing::warn!(%err, url, "failed to cache content");
        }
    }

    /// Route a mouse button press/release by region. A left press in the chrome
    /// band (toolbar + any open dropdown) hit-tests + dispatches the chrome; any
    /// other press in the content band, and every release, goes to the orrery in
    /// content-band coordinates (its viewport top sits at the toolbar bottom).
    fn on_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        let orrery_button = match button {
            MouseButton::Left => Some(PointerButton::Left),
            MouseButton::Middle => Some(PointerButton::Middle),
            MouseButton::Right => Some(PointerButton::Right),
            _ => None,
        };
        let (x, y) = self.cursor;
        let th = self.toolbar_height() as f32;
        match state {
            ElementState::Pressed => {
                // A context menu swallows the next press: a left click on one of its
                // rows runs that action (the chrome closes the menu); a click
                // anywhere else just dismisses it.
                if self.runner.state().context_menu.is_some() {
                    if button == MouseButton::Left {
                        self.chrome_click(x, y);
                    }
                    if self.runner.state().context_menu.is_some() {
                        self.close_context_menu();
                    }
                    return;
                }
                // The chrome's interactive area is the toolbar plus any open
                // dropdown (its `.chrome` border-box). A left press there dispatches
                // the chrome; below it (the content band) a right press opens the
                // context menu, anything else drives the orrery.
                let chrome_h = {
                    let dom = self.dom.borrow();
                    measure_class_bottom(&dom, self.width, self.height, "chrome")
                        .unwrap_or(self.toolbar_h.max(FALLBACK_TOOLBAR_H))
                };
                if y < chrome_h as f32 {
                    if button == MouseButton::Left {
                        self.chrome_click(x, y);
                    }
                } else if self.workbench.is_tiled() {
                    // Tree mode: the content band is the workbench root. A left press
                    // on a divider starts a resize; otherwise it routes to the root
                    // (tab switch / close / pin). The orrery is hidden.
                    if button == MouseButton::Left {
                        if let Some(i) = self.divider_at(x, y) {
                            self.divider_drag = Some((i, x, self.workbench.weights()));
                        } else {
                            self.workbench_click(x, y);
                        }
                    }
                } else if button == MouseButton::Right {
                    self.open_context_menu_at(x, y);
                } else if let Some(b) = orrery_button {
                    if self.orrery.pointer_down(b, x, y - th) {
                        self.request_redraw();
                    }
                }
            },
            ElementState::Released => {
                // Releases always reach the orrery: it acts only if it owns an
                // in-progress pan / drag / marquee, so a chrome-band release is a
                // harmless no-op. A click-release selects the node under the cursor.
                if let Some(b) = orrery_button {
                    if self.orrery.pointer_up(b, x, y - th) {
                        self.request_redraw();
                    }
                }
                // A divider resize ends on release.
                if button == MouseButton::Left {
                    self.divider_drag = None;
                }
                // Resolve a tab drag (tiled view): if the press moved past the slop
                // and released over a tile, drop by zone — the outer quarter on
                // either side splits the tab out to a new slot there, the center
                // moves / stacks it into that slot (reorder within, move across). A
                // release in place was a plain click (the tab activated on press).
                if button == MouseButton::Left {
                    if let Some((member, (px, py))) = self.tab_drag.take() {
                        if (x - px).hypot(y - py) > 6.0 {
                            if let Some((target, [x0, _, x1, _])) = self.tile_at(x, y) {
                                let edge = (x1 - x0).max(1.0) * 0.25;
                                let moved = if x < x0 + edge {
                                    self.workbench.split_beside(member, target, false)
                                } else if x > x1 - edge {
                                    self.workbench.split_beside(member, target, true)
                                } else {
                                    self.workbench.move_to_slot_of(member, target)
                                };
                                if moved {
                                    self.focused_tile = Some(member);
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }
                // A double-click on a node (in Cartography) opens the tiled workbench
                // from it: the first release selected it, so the working set is ready.
                if button == MouseButton::Left && !self.workbench.is_tiled() {
                    let now = Instant::now();
                    let double = self.last_left_release.is_some_and(|(t, (lx, ly))| {
                        now.duration_since(t) < Duration::from_millis(400)
                            && (x - lx).hypot(y - ly) < 6.0
                    });
                    self.last_left_release = Some((now, (x, y)));
                    if double && !self.orrery.selected_members().is_empty() {
                        self.last_left_release = None; // don't chain a triple-click
                        self.toggle_workbench();
                    }
                }
            },
        }
    }

    /// Hit-test the chrome root at `(x, y)` and dispatch the click (buttons +
    /// suggestion / palette rows). A row / backdrop click that closes the palette
    /// restores focus so the caret doesn't dangle on the removed field.
    fn chrome_click(&mut self, x: f32, y: f32) {
        let offsets = ScrollOffsets::<NodeId>::default();
        let hit = {
            let dom = self.dom.borrow();
            hit_test_node(&dom, CHROME_SHEET, self.width, self.height, x, y, &offsets)
        };
        if let Some(node) = hit {
            let palette_was_open = self.runner.state().palette_open;
            self.runner.dispatch_click(node, PointerClick::at((x, y)));
            self.drain_pending_connect();
            self.drain_pending_command();
            self.drain_pending_context();
            self.sync_settings();
            self.sync_orrery();
            if palette_was_open && !self.runner.state().palette_open {
                self.focus_after_palette_close();
            }
            self.request_redraw();
        }
    }

    /// Hit-test the workbench root at window `(x, y)` (content-band coords) and
    /// dispatch the click — a tab switch, a close, or a pin toggle. The action is
    /// captured in the workbench scene and drained immediately onto the model.
    fn workbench_click(&mut self, x: f32, y: f32) {
        let th = self.toolbar_height() as f32;
        let content_h = self.height.saturating_sub(self.toolbar_height()).max(1);
        let offsets = ScrollOffsets::<NodeId>::default();
        let hit = {
            let dom = self.workbench_dom.borrow();
            hit_test_node(&dom, WORKBENCH_SHEET, self.width, content_h, x, y - th, &offsets)
        };
        if let Some(node) = hit {
            self.workbench_runner.dispatch_click(node, PointerClick::at((x, y - th)));
            // A tab activated → remember it as a drag candidate (resolved on
            // release: a move when dragged onto another slot, else a plain click).
            if let Some(WorkbenchAction::Activate(member)) = self.workbench_runner.state().pending {
                self.tab_drag = Some((member, (x, y)));
            }
            self.drain_workbench_action();
            self.request_redraw();
        }
    }

    /// The tile (member + window rect) under `(x, y)` — the drag drop target, from
    /// this frame's laid-out tile rects.
    fn tile_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, [f32; 4])> {
        self.tile_rects
            .iter()
            .find(|(_, r)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3])
            .copied()
    }

    /// If `(x, y)` is over a divider (the gutter between two slots), its left-slot
    /// index — the start of a resize drag.
    fn divider_at(&mut self, x: f32, y: f32) -> Option<usize> {
        let th = self.toolbar_height() as f32;
        let content_h = self.height.saturating_sub(self.toolbar_height()).max(1);
        let offsets = ScrollOffsets::<NodeId>::default();
        let dom = self.workbench_dom.borrow();
        let node = hit_test_node(&dom, WORKBENCH_SHEET, self.width, content_h, x, y - th, &offsets)?;
        if !has_class(&dom, node, "wb-divider") {
            return None;
        }
        dom.attributes(node)
            .find(|a| a.name.local.as_ref() == "data-divider")
            .and_then(|a| a.value.parse::<usize>().ok())
    }

    /// Resize on a divider drag: shift width between the two slots the divider sits
    /// between, by the cursor's offset from the press as a fraction of the band.
    fn drag_divider(&mut self) {
        let Some((i, press_x, snapshot)) = self.divider_drag.clone() else {
            return;
        };
        if i + 1 >= snapshot.len() {
            return;
        }
        let band_w = self.width.max(1) as f32;
        let sum: f32 = snapshot.iter().sum();
        let dw = (self.cursor.0 - press_x) / band_w * sum;
        let mut weights = snapshot;
        weights[i] = (weights[i] + dw).max(0.05);
        weights[i + 1] = (weights[i + 1] - dw).max(0.05);
        self.workbench.set_weights(&weights);
        self.request_redraw();
    }

    /// While a tab is being dragged (moved past the slop), the member of the tile
    /// under the pointer — the highlighted drop target. `None` otherwise.
    fn drag_target_member(&self) -> Option<GraphMemberId> {
        let (_, (px, py)) = self.tab_drag?;
        let (cx, cy) = self.cursor;
        if (cx - px).hypot(cy - py) <= 6.0 {
            return None; // not dragging yet (still a click)
        }
        self.tile_at(cx, cy).map(|(m, _)| m)
    }

    /// Apply a pending workbench action the workbench root captured: switch the
    /// visible tab, close a tab (reaping its actor), or toggle its pin (the
    /// background-keep flag, which also exempts it from cap eviction).
    fn drain_workbench_action(&mut self) {
        let Some(action) = self.workbench_runner.state().pending else {
            return;
        };
        self.workbench_runner.update(|s| s.pending = None);
        match action {
            WorkbenchAction::Activate(member) => {
                self.workbench.activate(member);
                self.focused_tile = Some(member);
            },
            WorkbenchAction::Close(member) => {
                self.workbench.close_tile(member);
                self.constellation.reap(member);
                if self.focused_tile == Some(member) {
                    self.focused_tile = self.workbench.open_members().first().copied();
                }
            },
            WorkbenchAction::TogglePin(member) => {
                let pinned = self.constellation.is_background(member);
                self.constellation.set_background(member, !pinned);
            },
        }
        self.request_redraw();
    }

    /// Execute a pending "connect to peer" request the chrome queued (S5.1): take
    /// the ticket the verb captured from the address bar and drive the sync actor.
    /// The chrome records the intent; this is the host executing it.
    fn drain_pending_connect(&mut self) {
        let Some(ticket) = self.runner.state().pending_connect.clone() else {
            return;
        };
        self.runner.update(|c| {
            c.pending_connect = None;
        });
        if ticket.is_empty() {
            tracing::warn!("connect to peer: paste the peer's ticket in the address bar first");
            return;
        }
        match self.sync.connect(&ticket) {
            Ok(()) => tracing::info!("connect to peer: ticket accepted, overlay forming"),
            Err(err) => tracing::warn!(%err, "connect to peer failed"),
        }
        self.request_redraw();
    }

    /// Execute a pending host action the palette queued (toggle workbench / delete
    /// node / background a node): take it from the chrome and dispatch to the
    /// matching shell method. Mirrors [`drain_pending_connect`](Self::drain_pending_connect).
    fn drain_pending_command(&mut self) {
        let Some(cmd) = self.runner.state().pending_command else {
            return;
        };
        self.runner.update(|c| c.pending_command = None);
        match cmd {
            Command::ToggleWorkbench => self.toggle_workbench(),
            Command::DeleteNode => self.delete_focused_node(),
            Command::BackgroundNode => self.toggle_focus_background(),
            Command::HideSelectedEdge => {
                if self.orrery.hide_selected_edges() > 0 {
                    self.request_redraw();
                }
            },
            Command::ShowAllEdges => {
                if self.orrery.show_all_edges() > 0 {
                    self.request_redraw();
                }
            },
            // History / connect / settings verbs run in the chrome; never queued
            // here as host intents.
            Command::Back
            | Command::Forward
            | Command::Home
            | Command::ConnectPeer
            | Command::OpenSettings => {},
        }
    }

    /// Apply the chrome's current settings to the host: the active-tab cap to the
    /// actor pool. Called after a chrome interaction that could have changed them.
    /// Persists to the settings sidecar when the value actually changed (so an
    /// unrelated chrome click doesn't re-write the file).
    fn sync_settings(&mut self) {
        let cap = self.runner.state().settings.tab_cap;
        self.constellation.set_cap(cap);
        if cap != self.saved_tab_cap {
            self.saved_tab_cap = cap;
            self.persist_settings();
        }
    }

    /// Write the current settings to the session's `settings.json` sidecar. A
    /// failure is logged, not fatal (the shell runs without persistence).
    fn persist_settings(&self) {
        let settings = PersistedSettings { tab_cap: self.saved_tab_cap };
        if let Err(err) = settings_store::save_settings(&self.session_dir, &settings) {
            tracing::warn!(%err, "failed to persist settings");
        }
    }

    /// Handle a pressed key. Ctrl+K toggles the command palette; while the
    /// palette is open all keys route to it. Otherwise Enter submits the omnibar,
    /// Arrow Up/Down and Escape drive the suggestions dropdown, and every other
    /// key edits the omnibar and regenerates suggestions.
    fn on_key_pressed(&mut self, key: &WinitKey) {
        // An open context menu eats Escape to dismiss (other keys fall through).
        if self.runner.state().context_menu.is_some()
            && matches!(key, WinitKey::Named(WinitNamedKey::Escape))
        {
            self.close_context_menu();
            return;
        }
        // While the settings overlay is open, Escape closes it and other keys are
        // swallowed (clicks on its controls go through the chrome path).
        if self.runner.state().settings_open {
            if matches!(key, WinitKey::Named(WinitNamedKey::Escape)) {
                self.runner.update(Chrome::close_settings);
                self.request_redraw();
            }
            return;
        }
        if self.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("k"))
        {
            self.toggle_palette();
            return;
        }
        // Ctrl+T toggles the tiled workbench (Tree projection) and the orrery
        // (Cartography projection) of the same graph.
        if self.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("t"))
        {
            self.toggle_workbench();
            return;
        }
        // Ctrl+B flags the focused node to keep working in the background (its
        // actor outlives the view); Ctrl+Backspace deletes the focused node from
        // the graph. Both are modifier-gated so they don't collide with omnibar
        // editing, and intercepted here before the keystroke reaches the field.
        if self.modifiers.ctrl
            && matches!(key, WinitKey::Character(s) if s.eq_ignore_ascii_case("b"))
        {
            self.toggle_focus_background();
            return;
        }
        if self.modifiers.ctrl && matches!(key, WinitKey::Named(WinitNamedKey::Backspace)) {
            self.delete_focused_node();
            return;
        }
        if self.runner.state().palette_open {
            self.on_palette_key(key);
            return;
        }
        let suggestions_open = !self.runner.state().suggest.is_empty();
        match key {
            WinitKey::Named(WinitNamedKey::Enter) if self.runner.focus().is_some() => {
                self.runner.update(submit_omnibar);
                tracing::info!(
                    location = %self.runner.state().toolbar.editable.location,
                    "omnibar submit"
                );
                self.sync_orrery();
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowDown) if suggestions_open => {
                self.runner.update(|c| c.step_suggestion(1));
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowUp) if suggestions_open => {
                self.runner.update(|c| c.step_suggestion(-1));
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::Escape) if suggestions_open => {
                self.runner.update(Chrome::close_suggestions);
                self.request_redraw();
            },
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.modifiers) {
                    self.runner.dispatch_key(key_event);
                    self.runner.update(Chrome::refresh_suggestions);
                    self.request_redraw();
                }
            },
        }
    }

    /// Route a key to the open command palette: Enter runs the selection, Arrow
    /// Up/Down step it, Escape closes, anything else edits the query.
    fn on_palette_key(&mut self, key: &WinitKey) {
        match key {
            WinitKey::Named(WinitNamedKey::Enter) => {
                self.runner.update(Chrome::run_palette_selection);
                self.drain_pending_connect();
                self.drain_pending_command();
                self.sync_orrery();
                self.focus_after_palette_close();
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::Escape) => {
                self.runner.update(Chrome::close_palette);
                self.focus_after_palette_close();
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowDown) => {
                self.runner.update(|c| c.step_palette(1));
                self.request_redraw();
            },
            WinitKey::Named(WinitNamedKey::ArrowUp) => {
                self.runner.update(|c| c.step_palette(-1));
                self.request_redraw();
            },
            other => {
                if let Some(key_event) = key_event_from_winit(other, self.modifiers) {
                    self.runner.dispatch_key(key_event);
                    self.runner.update(Chrome::sync_palette_query);
                    self.request_redraw();
                }
            },
        }
    }

    /// Toggle the palette and move focus to match: into the palette query when
    /// it opens, back to the omnibar when it closes.
    fn toggle_palette(&mut self) {
        self.runner.update(Chrome::toggle_palette);
        if self.runner.state().palette_open {
            if let Some(node) = self.input_under_class("palette") {
                self.runner.set_focus(Some(node));
            }
        } else {
            self.focus_after_palette_close();
        }
        self.request_redraw();
    }

    /// Restore focus to the omnibar after the palette closes (so keyboard use
    /// continues there).
    fn focus_after_palette_close(&mut self) {
        let omnibar = self.input_under_class("toolbar");
        self.runner.set_focus(omnibar);
    }

    /// The first `<input>` under the first element carrying CSS class `class`
    /// (the omnibar under `.toolbar`, the query field under `.palette`).
    fn input_under_class(&self, class: &str) -> Option<NodeId> {
        let dom = self.dom.borrow();
        let container = first_with_class(&dom, dom.document(), class)?;
        first_tag(&dom, container, "input")
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Meerkat — Mere chrome on serval")
            .with_inner_size(PhysicalSize::new(self.width, self.height));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create meerkat window"),
        );
        let size = window.inner_size();
        self.width = size.width.max(1);
        self.height = size.height.max(1);

        // The shared serval-on-winit present stack: wgpu + netrender boot, surface
        // configured at the window size.
        let options =
            NetrenderOptions { tile_cache_size: Some(64), enable_vello: true, ..Default::default() };
        match SurfaceHost::boot(window.clone(), self.width, self.height, options) {
            Ok(host) => self.host = Some(host),
            Err(err) => {
                eprintln!("[meerkat] {err}");
                event_loop.exit();
                return;
            },
        }
        window.request_redraw();
        self.window = Some(window);

        // Show the restored focused node's content from the durable cache (so a
        // reload re-opens its card without a navigation). A fresh `mere://welcome`
        // focus is not fetchable, so this is a no-op there.
        if let Some(url) = self.orrery.focused_url().map(str::to_string) {
            self.ensure_content(&url);
        }
    }

    /// Drain completed fetches (delivery model 2): a worker woke us via the proxy;
    /// fold each outcome into the content cache and re-render the card.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
        // The kernel inbox dispatch: the one documented place that applies what the
        // actors tell the kernel. Each typed stream is drained and folded into
        // canonical state here on the kernel thread; the actors never touch it.
        let mut card_changed = false;
        let mut graph_changed = false;
        // One `FetchUpdate` stream carries both page documents and subresources.
        while let Ok(update) = self.inbox.fetch.try_recv() {
            match update {
                fetch::FetchUpdate::Page(outcome) => {
                    let state = match outcome.result {
                        Ok(fetched) => {
                            // Persist so a reload shows this page without
                            // re-fetching. Linked-data harvest now happens in the
                            // content actor (on `Show`), which ships a `Contribution`.
                            self.save_cached(
                                &outcome.url,
                                fetched.content_type.clone(),
                                fetched.body.as_bytes(),
                            );
                            fetch::ContentState::Ready(fetched)
                        },
                        Err(reason) => fetch::ContentState::Failed(reason),
                    };
                    self.content.insert(outcome.url, state);
                    card_changed = true;
                },
                // A subresource (page CSS / media): persist it (its content-type is
                // unknown here) so the page's assets survive restart, then broadcast
                // the bytes to every active node's actor — the fetch stream is keyed
                // by URL, not by which node wanted it; each actor dedups via its own
                // resource store and only the one that wanted it re-renders.
                fetch::FetchUpdate::Subresource(sub) => {
                    self.save_cached(&sub.url, None, &sub.bytes);
                    self.constellation.broadcast_resource(&sub.url, &sub.bytes);
                },
            }
        }
        // Drain every active node's actor in one pass: scenes land in the pool, the
        // wanted subresources + harvested contributions come back for the host.
        let drained = self.constellation.drain();
        card_changed |= drained.any_scene;
        if !drained.respawned.is_empty() {
            // A content tile's actor died (panic, isolated to its thread) and the
            // pool respawned it; redraw so the next frame re-Shows it (self-healing).
            tracing::warn!(count = drained.respawned.len(), "respawned crashed content tile(s)");
            card_changed = true;
        }
        for (member, urls) in drained.wanted {
            // The actor deduped these; a durable-cache hit feeds that node directly,
            // a miss spawns a network fetch whose bytes broadcast back on arrival.
            for url in urls {
                if let Some(stored) = self.load_cached(&url) {
                    self.constellation.send_resource(member, url, stored.body);
                } else {
                    self.fetch_handle.command(fetch::FetchCommand::Subresource(url));
                }
            }
        }
        if !drained.contributions.is_empty() {
            graph_changed |= self.orrery.ingest_graph(|g| {
                let mut changed = false;
                for contribution in &drained.contributions {
                    let outcome = linked_data::apply_contribution(g, contribution);
                    changed |= outcome.nodes_created > 0 || outcome.edges_asserted > 0;
                }
                changed
            });
        }
        // P2P sync status (S5.0): the same wake also carries lane-status changes.
        // Fold the latest into the chrome chip (the host owns the mutation).
        let mut latest_sync = None;
        while let Ok(update) = self.inbox.sync.try_recv() {
            latest_sync = Some(sync::to_indicator(&update.status, sync::LANE_LABEL));
        }
        if let Some(indicator) = latest_sync {
            self.runner.update(|c| c.sync = indicator.clone());
            self.request_redraw();
        }
        if graph_changed {
            self.save_session();
        }
        if card_changed || graph_changed {
            self.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        if self.window.as_ref().map(|w| w.id()) != Some(window_id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_session();
                event_loop.exit();
            },
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                // Forward to the orrery in content-band coordinates (Cartography
                // only — in the tiled view the orrery is hidden, so a stray drag
                // must not animate it and force every tile to re-rasterize).
                let th = self.toolbar_height() as f32;
                if self.divider_drag.is_some() {
                    self.drag_divider();
                } else if !self.workbench.is_tiled()
                    && self.orrery.cursor_moved(self.cursor.0, self.cursor.1 - th)
                {
                    self.request_redraw();
                } else if self.workbench.is_tiled() && self.tab_drag.is_some() {
                    // Follow the drag: the drop-target highlight tracks the pointer.
                    self.request_redraw();
                }
            },
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = modifiers_from_winit(mods.state());
                self.orrery.set_ctrl(self.modifiers.ctrl);
            },
            WindowEvent::MouseWheel { delta, .. } => {
                // Wheel over the content band drives the orrery (pan, or zoom under
                // Ctrl). LineDelta is scaled to device px the way the orrery
                // expects; PixelDelta passes through. Ignored in the tiled view (the
                // orrery is hidden; tiles don't scroll yet).
                let th = self.toolbar_height() as f32;
                if self.cursor.1 >= th && !self.workbench.is_tiled() {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            (x * WHEEL_PAN_SCALE, y * WHEEL_PAN_SCALE)
                        },
                        MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                    };
                    if self.orrery.wheel(dx, dy) {
                        self.request_redraw();
                    }
                }
            },
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_input(state, button),
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    self.on_key_pressed(&event.logical_key);
                }
            },
            WindowEvent::RedrawRequested => self.render(),
            _ => {},
        }
    }
}

/// Lay out the chrome root and return the border-box bottom (px, rounded up) of
/// the first element carrying CSS class `class` — `"toolbar"` for the content
/// split, `"chrome"` for the click-region gate (toolbar + open dropdown).
/// `None` if no such element is laid out.
fn measure_class_bottom(dom: &ScriptedDom, w: u32, h: u32, class: &str) -> Option<u32> {
    let frags = fragments_from_scripted_dom(dom, CHROME_SHEET, w, h);
    first_with_class(dom, dom.document(), class)
        .and_then(|node| frags.rect_of(node))
        .map(|layout| (layout.location.y + layout.size.height).ceil() as u32)
        .filter(|&measured| measured > 0)
}

/// The first element carrying CSS class `class` in pre-order under `id`.
fn first_with_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Option<NodeId> {
    if has_class(dom, id, class) {
        return Some(id);
    }
    dom.dom_children(id).find_map(|c| first_with_class(dom, c, class))
}

/// Every element carrying CSS class `class` in pre-order under `id`. Used to find
/// the workbench root's content placeholders, one per tile.
fn all_with_class(dom: &ScriptedDom, id: NodeId, class: &str) -> Vec<NodeId> {
    let mut out = Vec::new();
    if has_class(dom, id, class) {
        out.push(id);
    }
    for child in dom.dom_children(id) {
        out.extend(all_with_class(dom, child, class));
    }
    out
}

/// The `data-member` attribute of element `id`, parsed as a graph member id — the
/// tile whose content composites at this placeholder's rect.
fn member_attr(dom: &ScriptedDom, id: NodeId) -> Option<GraphMemberId> {
    dom.attributes(id)
        .find(|a| a.name.local.as_ref() == "data-member")
        .and_then(|a| a.value.parse::<GraphMemberId>().ok())
}

/// The first element with local tag `local` in pre-order under `id`.
fn first_tag(dom: &ScriptedDom, id: NodeId, local: &str) -> Option<NodeId> {
    if dom.element_name(id).is_some_and(|q| q.local.as_ref() == local) {
        return Some(id);
    }
    dom.dom_children(id).find_map(|c| first_tag(dom, c, local))
}

/// Whether element `id` carries CSS class `class` (whitespace-split `class` attr).
fn has_class(dom: &ScriptedDom, id: NodeId, class: &str) -> bool {
    dom.attributes(id).any(|attr| {
        attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
    })
}

/// Map the orrery camera (pan + zoom) to a serialized [`CameraSnapshot`] — the
/// kurbo `Affine` coefficient order `[a, b, c, d, e, f]`. The orrery camera is a
/// pure scale + translate (no rotation / skew), so `a = d = zoom`, `b = c = 0`,
/// and `(e, f)` is the offset.
fn camera_to_snapshot(camera: CameraView) -> CameraSnapshot {
    let zoom = camera.zoom as f64;
    CameraSnapshot {
        coefficients: [zoom, 0.0, 0.0, zoom, camera.offset.0 as f64, camera.offset.1 as f64],
    }
}

/// The inverse of [`camera_to_snapshot`]: recover pan + zoom from the affine
/// coefficients (scale from `a`, offset from `e` / `f`; rotation / skew are
/// ignored, as the orrery never sets them).
fn snapshot_to_camera(snapshot: &CameraSnapshot) -> CameraView {
    let c = snapshot.coefficients;
    CameraView { offset: (c[4] as f32, c[5] as f32), zoom: c[0] as f32 }
}

/// A durably-cached entry as a [`fetch::Fetched`], decoding the stored body as
/// text (lossily). Binary subresources are served from the resource cache as
/// bytes; this text view is for the page-document lane.
fn fetched_from(stored: content_store::StoredContent) -> fetch::Fetched {
    fetch::Fetched {
        content_type: stored.content_type,
        body: String::from_utf8_lossy(&stored.body).into_owned(),
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("meerkat=info")),
        )
        .init();
    tracing::info!("meerkat-shell starting");

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("event loop error");
}
