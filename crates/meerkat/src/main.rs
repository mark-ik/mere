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
use std::time::Instant;

use eidetic_fjall::FjallStore;
use forme::GraphMemberId;
use inker::EngineRegistry;
use layout_dom_api::LayoutDom;
use meerkat::{chrome_view, Chrome, ChromeLogic, ChromeView};
use orrery::{CameraView, Orrery};
use pelt_live::fragments_from_scripted_dom;
use platen::Workbench;
use platen_view::{workbench_view, WorkbenchLogic, WorkbenchScene, WorkbenchTreeView};
use register_theme::chrome::{ChromeTheme, Color32};
use register_theme::theme::ThemeRegistry;
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::SurfaceHost;
use session_runtime::{session_graph_store, settings_store, view_intent_store};
use winit::window::{CursorIcon, ResizeDirection};
use xilem_serval::{Modifiers, ServalAppRunner};

mod card;
mod comms_host;
mod constellation;
mod content;
mod fetch;
mod resources;
mod sync;

mod app_handler;
mod frame_ops;
mod input;
mod render;
mod titlebar;

use constellation::Constellation;

/// Build the chrome root's author CSS from a resolved [`ChromeTheme`] (theming
/// pass). The toolbar is a flex row (back / forward buttons + a growing omnibar)
/// that serval lays out via taffy's flexbox; the `.chrome` container itself has
/// no background, so the host composites it over the content root and only the
/// toolbar + the (opaque) dropdowns paint over the page. The toolbar band sits a
/// step above the graph backdrop, fields/buttons a step above that, dropdowns +
/// floated panels their own tiers; all colors come from the active theme so the
/// chrome theme-switches alongside the graph. Surfaces, not classes, are the
/// unit: the toolbar, palette, settings, and comms pane reuse the same dozen
/// tokens, so a theme reads as one coherent shell.
fn chrome_sheet(c: &ChromeTheme) -> Vec<String> {
    let rgb = |color: Color32| {
        let [r, g, b, _] = color.to_array();
        format!("rgb({r}, {g}, {b})")
    };
    vec![
        "div, button, input { display: block; }".to_string(),
        // The toolbar reserves right padding the width of the window-control strip
        // (the borderless titlebar's min / max / close), so the omnibar + sync chip
        // stop short of it and the host composites the controls into that gap.
        format!(
            ".toolbar {{ display: flex; background-color: {}; padding: 8px {}px 8px 8px; }}",
            rgb(c.toolbar_bg),
            titlebar::CONTROLS_W as u32
        ),
        format!(
            "button {{ font-size: 22px; color: {}; background-color: {}; padding: 8px 14px; margin: 4px; }}",
            rgb(c.control_text), rgb(c.control_bg)
        ),
        format!(".disabled {{ color: {}; background-color: {}; }}", rgb(c.disabled_text), rgb(c.disabled_bg)),
        format!(
            "input {{ font-size: 22px; color: {}; background-color: {}; padding: 8px; margin: 4px; flex-grow: 1; }}",
            rgb(c.field_text), rgb(c.field_bg)
        ),
        // The p2p sync chip: small + muted, no flex-grow, so the omnibar pushes it
        // to the toolbar's right edge.
        format!(
            ".sync-chip {{ font-size: 14px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px; }}",
            rgb(c.muted_text), rgb(c.menu_bg)
        ),
        format!(".suggestions {{ background-color: {}; padding-bottom: 6px; }}", rgb(c.panel_bg)),
        format!(
            ".suggestion {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 16px; }}",
            rgb(c.body_text), rgb(c.panel_bg)
        ),
        format!(
            ".suggestion-active {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 16px; }}",
            rgb(c.strong_text), rgb(c.active_bg)
        ),
        // Command palette: a centered panel floated over the page (flex centering;
        // serval maps justify-content through stylo_taffy).
        ".palette-overlay { display: flex; justify-content: center; padding-top: 56px; }".to_string(),
        format!(".palette {{ width: 540px; background-color: {}; padding: 10px; }}", rgb(c.surface_bg)),
        format!(".cmd-list {{ background-color: {}; }}", rgb(c.surface_bg)),
        format!(
            ".cmd-row {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.body_text), rgb(c.surface_bg)
        ),
        format!(
            ".cmd-row-active {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 12px; }}",
            rgb(c.strong_text), rgb(c.active_bg)
        ),
        // Settings overlay: a centered panel (like the palette) with rows of controls.
        ".settings-overlay { display: flex; justify-content: center; padding-top: 56px; }".to_string(),
        format!(".settings {{ width: 380px; background-color: {}; padding: 14px; }}", rgb(c.surface_bg)),
        format!(".set-title {{ display: flex; background-color: {}; padding: 4px 4px 12px 4px; }}", rgb(c.surface_bg)),
        format!(
            ".set-title-text {{ font-size: 20px; color: {}; background-color: {}; flex-grow: 1; padding: 4px 8px; }}",
            rgb(c.strong_text), rgb(c.surface_bg)
        ),
        format!(".set-row {{ display: flex; background-color: {}; padding: 6px 8px; }}", rgb(c.surface_bg)),
        format!(
            ".set-value {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 14px; flex-grow: 1; }}",
            rgb(c.body_text), rgb(c.surface_bg)
        ),
        format!(
            ".set-btn {{ font-size: 20px; color: {}; background-color: {}; padding: 6px 16px; margin: 0 4px; }}",
            rgb(c.control_text), rgb(c.control_bg)
        ),
        // Right-click context menu: a small panel of action rows floated at the cursor.
        format!(".context-menu {{ background-color: {}; padding: 4px; }}", rgb(c.menu_bg)),
        format!(
            ".context-item {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 18px; }}",
            rgb(c.body_text), rgb(c.menu_bg)
        ),
        // Comms pane (P6): a right-edge docked panel over the content. (Geometry is
        // a first cut — top offset + right dock get tuned on the first run.)
        format!(
            ".comms-pane {{ position: absolute; top: 64px; right: 0; width: 360px; background-color: {}; padding: 10px; }}",
            rgb(c.panel_bg)
        ),
        format!(".comms-title {{ display: flex; background-color: {}; padding: 4px 4px 10px 4px; }}", rgb(c.panel_bg)),
        format!(
            ".comms-title-text {{ font-size: 20px; color: {}; background-color: {}; flex-grow: 1; padding: 4px 8px; }}",
            rgb(c.strong_text), rgb(c.panel_bg)
        ),
        format!(
            ".comms-btn {{ font-size: 18px; color: {}; background-color: {}; padding: 4px 12px; }}",
            rgb(c.control_text), rgb(c.control_bg)
        ),
        format!(
            ".comms-failure {{ font-size: 14px; color: {}; background-color: {}; padding: 6px 10px; margin-bottom: 6px; }}",
            rgb(c.error_text), rgb(c.error_bg)
        ),
        format!(
            ".comms-row {{ font-size: 17px; color: {}; background-color: {}; padding: 10px 12px; margin: 3px 0; }}",
            rgb(c.body_text), rgb(c.surface_bg)
        ),
        format!(
            ".comms-empty {{ font-size: 15px; color: {}; background-color: {}; padding: 10px 12px; }}",
            rgb(c.muted_text), rgb(c.panel_bg)
        ),
        format!(
            ".comms-back {{ font-size: 15px; color: {}; background-color: {}; padding: 6px 12px; margin-bottom: 6px; }}",
            rgb(c.body_text), rgb(c.control_bg)
        ),
        format!(".comms-thread-title {{ font-size: 18px; color: {}; background-color: {}; padding: 8px 4px; }}", rgb(c.strong_text), rgb(c.panel_bg)),
        format!(
            ".comms-msg-in {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 24px 4px 0; }}",
            rgb(c.body_text), rgb(c.menu_bg)
        ),
        format!(
            ".comms-msg-out {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 12px; margin: 4px 0 4px 24px; }}",
            rgb(c.strong_text), rgb(c.active_bg)
        ),
        format!(".comms-compose {{ display: flex; background-color: {}; padding-top: 8px; }}", rgb(c.panel_bg)),
        format!(
            ".comms-send {{ font-size: 16px; color: {}; background-color: {}; padding: 8px 16px; margin: 4px; }}",
            rgb(c.control_text), rgb(c.active_bg)
        ),
    ]
}

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
    /// The p2p sync actor's command handle (S5.0 / S5.1). The actor owns the
    /// transport + tessera lane on its own tokio runtime; status arrives on
    /// `inbox.sync`, and the "connect to peer" verb is a `SyncCommand` sent here.
    sync_handle: armillary::ActorHandle<sync::SyncCommand>,
    /// The comms actor's command handle (P6c). The actor owns the live `Comms`
    /// (misfin + murm adapters) on its own tokio runtime; conversation lists +
    /// threads arrive on `inbox.comms`, and load / send verbs are `CommsCommand`s
    /// sent here.
    comms_handle: armillary::ActorHandle<comms_host::CommsCommand>,
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
    window: Option<Arc<winit::window::Window>>,
    /// The shared serval-on-winit present stack, built once a window exists.
    host: Option<SurfaceHost>,
    /// Tracked keyboard modifiers, folded into each dispatched `KeyEvent`.
    modifiers: Modifiers,
    /// System clipboard for the omnibar / palette Ctrl(Cmd)+C/X/V. `None` if the
    /// platform clipboard could not be opened (the shortcuts then no-op).
    clipboard: Option<arboard::Clipboard>,
    /// Last cursor position in physical pixels (window space == content space).
    cursor: (f32, f32),
    /// The last left-button release (time + window pos), for double-click detection.
    /// A double-click on an orrery node opens the tiled workbench from it.
    last_left_release: Option<(Instant, (f32, f32))>,
    /// The tile in focus in the tiled view (the last activated / opened member), so
    /// the omnibar can show its URL. `None` outside the tiled view or with no tiles.
    focused_tile: Option<GraphMemberId>,
    /// Nodes promoted to a *live* preview card (double-clicked up from their "last
    /// visit" snapshot). Drives `needed_members` in Cartography, so a node is active
    /// only when it has a live preview (or a tile) — focusing alone shows the static
    /// snapshot with no actor. (Card system P2/P3.)
    live_previews: std::collections::HashSet<GraphMemberId>,
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
    /// Per-member content scroll offset (px from the document top). A card
    /// composites a window of its full-height texture at this offset; a wheel over
    /// the card adjusts it (clamped to the content height). Absent = scrolled to top.
    scroll: HashMap<GraphMemberId, f32>,
    /// Each composited card/tile's on-screen content rect this frame
    /// (member, [x0, y0, x1, y1]) — rebuilt every frame, used to route a wheel over
    /// a card to its scroll rather than to the orrery.
    content_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Cached rasterized close (X) button texture, shared across live cards; built
    /// once and composited at each live card's top-right corner. (Card system.)
    close_button_tex: Option<CachedTile>,
    /// Cached rasterized "unvisited" placeholder card (dashed outline + "Double-
    /// click to load"), shown when a focused node has no snapshot yet. (Card #3.)
    unvisited_tex: Option<CachedTile>,
    /// The nematic engine registry, for rendering "last visit" snapshot cards
    /// host-side from the durable content cache (no actor) — the same registry the
    /// content actor builds, kept here for the snapshot path. (Card #4.)
    engine_registry: EngineRegistry,
    /// Cached rasterized snapshot textures, keyed by URL (the snapshot is the
    /// node's last-visit content rendered from cache / synthesis). Re-rendered on
    /// a size change; persists the "last visit" look across the session. (Card #4.)
    snapshot_textures: HashMap<String, CachedTile>,
    /// Each live card's close-button rect this frame (member, [x0, y0, x1, y1]); a
    /// press inside reaps that live preview. Rebuilt every frame.
    close_button_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// An in-progress divider drag: the left-slot index, the press x, and the slot
    /// weights snapshot at press. Cursor moves reweight the two neighbouring slots.
    divider_drag: Option<(usize, f32, Vec<f32>)>,
    /// The active theme's chrome CSS (built from a resolved [`ChromeTheme`] at
    /// startup). The render / measure / hit-test paths read it instead of a const,
    /// so a theme switch rebuilds it and the whole shell re-themes. (Theming pass.)
    chrome_sheet: Vec<String>,
    /// The active theme's chrome tokens — kept beside the baked `chrome_sheet` for
    /// the host-drawn surfaces the CSS can't reach (the window-control glyphs).
    chrome_theme: ChromeTheme,
    /// An in-progress titlebar press (window point) on the borderless window: set
    /// on a left press in the toolbar bar's draggable area, cleared into a window
    /// drag once the pointer moves past the slop, else resolved as a click on
    /// release. (Custom titlebar.)
    titlebar_press: Option<(f32, f32)>,
    /// Set by the custom close control; the event handler exits the loop (saving
    /// the session) after the press is processed, since input has no event-loop
    /// handle. (Custom titlebar.)
    pending_exit: bool,
    /// Cached rasterized window-control strip (min / max / close), composited over
    /// the chrome at the toolbar's top-right. Re-rasterized on a band-size change.
    window_controls_tex: Option<CachedTile>,
    /// An in-progress manual window resize from a window edge / corner (custom
    /// titlebar). `None` outside a resize drag. (Custom titlebar.)
    resize_drag: Option<ResizeDrag>,
    /// The cursor icon currently set on the window — tracked so a hover over a
    /// resize edge only calls `set_cursor` on a change, not every move. (Custom
    /// titlebar.)
    cursor_icon: CursorIcon,
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

/// An in-progress manual window resize (custom titlebar). winit's
/// `drag_resize_window` is inert on frameless Windows — the non-client area is
/// removed via `WM_NCCALCSIZE`, so the OS has no edge frame to grab — so the host
/// resizes the window itself: it anchors the opposite edge(s) to the press-time
/// rect and tracks the cursor in screen space (the press-time screen cursor is the
/// origin, so there is no first-move jump). On Wayland `set_outer_position` is a
/// no-op, so left/top edges there can't move the origin; right/bottom still size.
#[derive(Clone, Copy)]
struct ResizeDrag {
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
    sync: Receiver<sync::SyncUpdate>,
    comms: Receiver<comms_host::CommsUpdate>,
}

impl App {
    fn new(proxy: winit::event_loop::EventLoopProxy<()>) -> Self {
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
        // Always-offload physics (P6): move the orrery's gyre simulation onto its
        // own armillary actor thread, so a heavy settle never blocks compositing or
        // input. It wakes the loop through the same winit proxy as the other
        // actors; the host folds each layout snapshot into the orrery's read model
        // on the next frame.
        let physics_proxy = proxy.clone();
        let physics_wake: armillary::Wake = Arc::new(move || {
            let _ = physics_proxy.send_event(());
        });
        orrery.offload_physics(physics_wake);
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
        // The p2p sync actor: an armillary actor whose run closure owns a tokio
        // runtime (built on its thread) that binds the transport + joins the tessera
        // demo moot, polling status back through the same wake shape as fetch/content.
        // Setup failure disables p2p, not the shell.
        let sync_proxy = proxy.clone();
        let sync_wake: armillary::Wake = Arc::new(move || {
            let _ = sync_proxy.send_event(());
        });
        let (sync_handle, sync_rx) = sync::spawn_sync(sync_wake, sync::DEMO_MOOT);
        // The comms actor: owns the live `Comms` (misfin + murm adapters over local
        // stores under the session dir) on its own tokio runtime, waking the loop
        // through the same winit proxy. Setup failure disables comms, not the shell.
        let comms_proxy = proxy.clone();
        let comms_wake: armillary::Wake = Arc::new(move || {
            let _ = comms_proxy.send_event(());
        });
        let (comms_handle, comms_rx) = comms_host::spawn_comms(comms_wake, session_dir.clone());
        // The host's own nematic engine registry, for rendering snapshot cards
        // from the durable cache without a live actor (Card #4).
        let mut engine_registry = EngineRegistry::new();
        for engine in nematic::engines() {
            engine_registry.register(engine);
        }
        // Resolve the active theme's chrome tokens once and bake the chrome CSS
        // from them (theming pass). A runtime theme switch (settings / apparatus)
        // rebuilds this from the registry; today it opens on the default theme.
        let theme = ThemeRegistry::default();
        let chrome_theme = theme.active_theme().tokens.chrome;
        let chrome_sheet = chrome_sheet(&chrome_theme);
        Self {
            dom,
            runner,
            orrery,
            content_location,
            centered: restored_camera.is_some(),
            fetch_handle,
            constellation,
            sync_handle,
            comms_handle,
            content: HashMap::new(),
            session_dir,
            store,
            toolbar_h: 0,
            window: None,
            host: None,
            modifiers: Modifiers::default(),
            clipboard: arboard::Clipboard::new().ok(),
            cursor: (0.0, 0.0),
            last_left_release: None,
            live_previews: std::collections::HashSet::new(),
            focused_tile: None,
            shown_location: None,
            tab_drag: None,
            tile_rects: Vec::new(),
            tile_textures: HashMap::new(),
            scroll: HashMap::new(),
            content_rects: Vec::new(),
            close_button_tex: None,
            unvisited_tex: None,
            engine_registry,
            snapshot_textures: HashMap::new(),
            close_button_rects: Vec::new(),
            divider_drag: None,
            chrome_sheet,
            chrome_theme,
            titlebar_press: None,
            pending_exit: false,
            window_controls_tex: None,
            resize_drag: None,
            cursor_icon: CursorIcon::Default,
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
            inbox: KernelInbox { fetch: fetch_rx, sync: sync_rx, comms: comms_rx },
            _kernel: armillary::KernelThread::new(),
        }
    }

    /// Request a redraw if a window exists.
    fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// The active theme's chrome CSS as `&[&str]`, the shape the serval layout /
    /// paint / hit-test entry points take. Borrows the baked `chrome_sheet`.
    fn chrome_sheet_refs(&self) -> Vec<&str> {
        self.chrome_sheet.iter().map(String::as_str).collect()
    }
}

/// Lay out the chrome root and return the border-box bottom (px, rounded up) of
/// the first element carrying CSS class `class` — `"toolbar"` for the content
/// split, `"chrome"` for the click-region gate (toolbar + open dropdown).
/// `None` if no such element is laid out.
fn measure_class_bottom(
    dom: &ScriptedDom,
    sheet: &[&str],
    w: u32,
    h: u32,
    class: &str,
) -> Option<u32> {
    let frags = fragments_from_scripted_dom(dom, sheet, w, h);
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
fn camera_to_snapshot(camera: CameraView) -> session_runtime::CameraSnapshot {
    let zoom = camera.zoom as f64;
    session_runtime::CameraSnapshot {
        coefficients: [zoom, 0.0, 0.0, zoom, camera.offset.0 as f64, camera.offset.1 as f64],
    }
}

/// The inverse of [`camera_to_snapshot`]: recover pan + zoom from the affine
/// coefficients (scale from `a`, offset from `e` / `f`; rotation / skew are
/// ignored, as the orrery never sets them).
fn snapshot_to_camera(snapshot: &session_runtime::CameraSnapshot) -> CameraView {
    let c = snapshot.coefficients;
    CameraView { offset: (c[4] as f32, c[5] as f32), zoom: c[0] as f32 }
}

/// A durably-cached entry as a [`fetch::Fetched`], decoding the stored body as
/// text (lossily). Binary subresources are served from the resource cache as
/// bytes; this text view is for the page-document lane.
fn fetched_from(stored: session_runtime::content_store::StoredContent) -> fetch::Fetched {
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

    let event_loop = winit::event_loop::EventLoop::new().expect("failed to create event loop");
    let proxy = event_loop.create_proxy();
    let mut app = App::new(proxy);
    event_loop.run_app(&mut app).expect("event loop error");
}
