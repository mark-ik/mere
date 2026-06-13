/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-window view state — the part of the shell that belongs to **one** OS
//! window, as opposed to the shared session state (graph, actors, caches,
//! manifests) that every window draws from. The seam this carves is the
//! foundation for multi-window tear-out: a second window is a second
//! [`WindowView`] over the same shared state.
//!
//! Built up cluster by cluster (MW1). First in: the per-frame **hit-rect
//! caches** — each is cleared and repopulated every render, then read by the
//! input path to route a press to whatever sits under the cursor. They are
//! pure view state (geometry of *this* window's surface this frame), so they
//! move here first. (Multi-window plan MW1.)

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use forme::GraphMemberId;
use frame::{FrameLayout, GraphId, PaneId, SessionId, SplitAxis, SplitChoice};
use meerkat::{Chrome, ChromeLogic, ChromeView};
use platen::Workbench;
use platen_view::{WorkbenchLogic, WorkbenchScene, WorkbenchTreeView};
use serval_scripted_dom::ScriptedDom;
use serval_winit_host::WindowSurface;
use winit::window::CursorIcon;
use xilem_serval::{Modifiers, ServalAppRunner};

use super::{CachedTile, ContentPane, ResizeDrag};
use crate::pane_session::PaneSession;

/// What a window *is*, which selects its chrome template (and, from MW6, its camera
/// ownership). The **primary** owns the orrery + the shellbar/switcher chrome and
/// saves the session on close. A **leaf** is a torn-out tile: slim chrome (no
/// shellbar, no switcher), workbench-only content over the shared graph, and closing
/// it just drops the view. The orrery-owning payload (`Primary(OrreryView)`) arrives
/// when MW6 splits the camera per-window; for now the variants are bare markers.
/// (Multi-window MW3 step 4.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WindowKind {
    Primary,
    Leaf,
}

impl WindowKind {
    /// Whether this window uses the slim chrome template (a leaf: no shellbar, no
    /// switcher). Read by the chrome sync + render + input band so all three agree.
    pub(crate) fn is_slim(self) -> bool {
        matches!(self, WindowKind::Leaf)
    }
}

/// State owned by a single window's view. Methods on `Shell` reach it through
/// `self.view`; when the window registry lands (MW2) the render / input paths
/// take `&mut WindowView` for the target window explicitly.
///
/// Constructed via [`WindowView::new`] — the chrome + workbench runners it owns
/// are `!Default` (a serval document authority can't be conjured), so the
/// derive-`Default` era ends here and a second window is minted by handing
/// [`WindowView::new`] a fresh pair of runners over the shared session. (MW2.)
pub(crate) struct WindowView {
    /// What this window is (primary / leaf) — selects the chrome template and, from
    /// MW6, camera ownership. Fixed at construction. (MW3 step 4.)
    pub(crate) kind: WindowKind,

    // ── Chrome authority: this window's two serval document roots and the runners
    //    that drive them — the toolbar / omnibar / dropdowns (chrome) and the tiled
    //    workbench. Separate roots by discipline; both per-window. (MW2.) ──────────
    /// The chrome DOM the runner mutates and the render path reads.
    pub(crate) dom: Rc<RefCell<ScriptedDom>>,
    /// The chrome runner: this window's toolbar / omnibar / dropdown authority.
    pub(crate) runner: ServalAppRunner<Chrome, ChromeLogic, ChromeView>,
    /// The chrome DOM's incremental cascade+layout session (cheap-path C3). `None`
    /// until the first render builds it; rebuilt on a structural / resize / theme
    /// change, else the per-frame attribute batch applies on the `RepaintOnly` path.
    pub(crate) chrome_session: Option<PaneSession>,
    /// The tiled-workbench composition (S4): the open tiles + the projection mode
    /// (Cartography = the orrery, Tree = the tiled view).
    pub(crate) workbench: Workbench,
    /// The workbench root: a second serval document authority (separate from the
    /// chrome root) that renders the tile tree as flex DOM via `platen_view`.
    pub(crate) workbench_dom: Rc<RefCell<ScriptedDom>>,
    /// The workbench runner driving `workbench_dom` from `workbench`.
    pub(crate) workbench_runner: ServalAppRunner<WorkbenchScene, WorkbenchLogic, WorkbenchTreeView>,
    /// The workbench DOM's incremental cascade+layout session (cheap-path C5) — the
    /// pane render, its slot-placement fragment reads, and its click hit-tests all
    /// share this one layout per frame. `None` until the workbench first renders.
    pub(crate) workbench_session: Option<PaneSession>,
    /// The roster pane as a view-driven bundle (runner + cached layout + sheet): a
    /// `roster_view` over its rows, dispatching row clicks through the DOM instead of
    /// a rect cache. (Window composition P2 companion — list-pane view-ification.)
    pub(crate) roster_pane: crate::roster_view::RosterPane,

    /// Each switcher row's on-screen rect this frame: a click switches to it.
    pub(crate) session_row_rects: Vec<(SessionId, [f32; 4])>,
    /// Each switcher row's close (×) hit rect this frame: a click trashes it.
    pub(crate) session_close_rects: Vec<(SessionId, [f32; 4])>,
    /// The "+" new-graph tile rect this frame, if the switcher is shown.
    pub(crate) session_add_rect: Option<[f32; 4]>,
    /// Each apparatus theme-button's rect this frame (theme id): a press switches.
    pub(crate) apparatus_button_rects: Vec<(String, [f32; 4])>,
    /// Each gloss minimap node's rect this frame (node id): a press focuses it.
    pub(crate) gloss_node_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each open tile's content rect this frame (member): the drag resolves its
    /// drop target + zone against it.
    pub(crate) tile_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each composited card/tile's on-screen content rect this frame (member):
    /// routes a wheel over a card to its scroll rather than the orrery.
    pub(crate) content_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each live card's close-button rect this frame (member): a press reaps that
    /// live preview.
    pub(crate) close_button_rects: Vec<(GraphMemberId, [f32; 4])>,

    // ── Paint caches: GPU textures rasterized for this window's surface, reused
    //    across frames while their version + size hold. ────────────────────────
    /// Cached rasterized texture per tile, keyed by member; evicted on close.
    pub(crate) tile_textures: HashMap<GraphMemberId, CachedTile>,
    /// Cached rasterized close (×) button texture, shared across live cards.
    pub(crate) close_button_tex: Option<CachedTile>,
    /// Cached rasterized "unvisited" placeholder card.
    pub(crate) unvisited_tex: Option<CachedTile>,
    /// Cached rasterized snapshot textures, keyed by URL (the node's last-visit look).
    pub(crate) snapshot_textures: HashMap<String, CachedTile>,
    /// Cached rasterized window-control strip (min / max / close).
    pub(crate) window_controls_tex: Option<CachedTile>,
    /// A small solid texture filling the frame-divider gutters between split panes.
    pub(crate) divider_tex: Option<CachedTile>,

    // ── Interaction state: in-progress gestures + scroll + transient view bits
    //    that belong to this window's pointer / keyboard, not the shared session. ─
    /// Per-member content scroll offset (px from the document top). Absent = top.
    pub(crate) scroll: HashMap<GraphMemberId, f32>,
    /// Roster pane scroll offset (device px), clamped at render.
    pub(crate) roster_scroll: f32,
    /// The last left-button release (time + window pos), for double-click detection.
    pub(crate) last_left_release: Option<(Instant, (f32, f32))>,
    /// An in-progress workbench tab drag: the pressed tab's member + press position.
    pub(crate) tab_drag: Option<(GraphMemberId, (f32, f32))>,
    /// An in-progress workbench slot-divider drag: left-slot index, press x, weights.
    pub(crate) divider_drag: Option<(usize, f32, Vec<f32>)>,
    /// An in-progress frame-divider drag: split path, parent rect, axis.
    pub(crate) frame_divider_drag: Option<(Vec<SplitChoice>, [f32; 4], SplitAxis)>,
    /// An in-progress manual window resize from an edge / corner (custom titlebar).
    pub(crate) resize_drag: Option<ResizeDrag>,
    /// An in-progress titlebar press (window point) before it becomes a window drag.
    pub(crate) titlebar_press: Option<(f32, f32)>,
    /// The cursor icon currently set on the window (tracked to set only on change).
    pub(crate) cursor_icon: CursorIcon,
    /// Set by the custom close control; the event handler exits the loop after the
    /// press is processed (input has no event-loop handle).
    pub(crate) pending_exit: bool,
    /// The members the open right-click context menu acts on (its working set).
    pub(crate) context_set: Vec<GraphMemberId>,
    /// In-progress session rename: the target session + its edit buffer. `Some` while
    /// the switcher label is being typed (F2 / right-click a tile).
    pub(crate) renaming: Option<(SessionId, String)>,

    // ── View-session state: what this window is looking at within the shared
    //    graph (its focus, its live cards, its nav target). ─────────────────────
    /// Whether this window's orrery has been centered on its content band yet.
    pub(crate) centered: bool,
    /// Whether the one-shot camera-restore self-heal has run for this window: it
    /// recenters once if a restored camera frames nothing (a degenerate saved
    /// pan/zoom), so a bad saved camera recovers on launch. (Orrery recovery.)
    pub(crate) healed: bool,
    /// The navigation target last synced into the orrery via `visit`; guards
    /// re-visiting. Mirrors this window's chrome `content_location`.
    pub(crate) content_location: String,
    /// The location last pushed into the omnibar by focus-follow, so it only updates
    /// when the focused tile / node actually changes (not every frame).
    pub(crate) shown_location: Option<String>,
    /// The tile in focus in this window's tiled view (the last activated / opened
    /// member), so the omnibar can show its URL. `None` outside Tree / with no tiles.
    pub(crate) focused_tile: Option<GraphMemberId>,
    /// Nodes promoted to a *live* preview card in this window (double-clicked up from
    /// their snapshot). Drives `needed_members` in Cartography.
    pub(crate) live_previews: HashSet<GraphMemberId>,

    // ── Frame: this window's pane arrangement + its companions. ──────────────────
    /// The content region's split tree of resizable panes (window-scoped, MG5).
    pub(crate) frame_layout: FrameLayout,
    /// Next pane id to mint when summoning a sibling pane in this window.
    pub(crate) next_pane_id: u64,
    /// The leaf maximized to the whole content band, if any (the maximize toggle).
    pub(crate) maximized_pane: Option<PaneId>,
    /// Which content pane navigation acts on (the last-interacted one).
    pub(crate) active_content: ContentPane,
    /// The graph this window's nav / focus / selection currently act on — the active
    /// content pane's graph. Resolves which pooled orrery the ctx bundles as
    /// `self.orrery`. At one graph per window it tracks the active session's graph;
    /// session-switch updates it. (Window composition P1.)
    pub(crate) focused_graph: GraphId,

    // ── Window + surface + size + input: the OS window itself, its present stack,
    //    its surface dimensions, and the pointer / modifier state of its focus.
    //    All per-window by definition — a second window is a second of each. (MW2.) ─
    /// This view's OS window. `None` until the window is created on resume.
    pub(crate) window: Option<Arc<winit::window::Window>>,
    /// This window's swapchain surface, created from the shared [`RenderCore`] once
    /// the OS window exists. The device behind it lives on `Shell.render_core`, shared
    /// across windows. `None` until resume. (MW3: one device, N surfaces.)
    pub(crate) surface: Option<WindowSurface>,
    /// Cached measured height (px) of this window's chrome band; `0` until measured.
    pub(crate) toolbar_h: u32,
    /// This window's surface width (physical px).
    pub(crate) width: u32,
    /// This window's surface height (physical px).
    pub(crate) height: u32,
    /// Last cursor position in this window (physical px; window space == content space).
    pub(crate) cursor: (f32, f32),
    /// Keyboard modifiers tracked for this window's focus, folded into each dispatched
    /// `KeyEvent`.
    pub(crate) modifiers: Modifiers,

    // ── Compatibility view (scrying): per-window, because each WebView is bound to
    //    this window's HWND. (Scrying X1/X2.) ──────────────────────────────────────
    /// The scrying pool: system-WebView tiles on the UI thread, beside the
    /// constellation.
    pub(crate) scrying: super::scrying_host::ScryingHost,
    /// The focused scrying tile's (member, window rect) this frame, set by render;
    /// the input path hit-tests it to forward mouse / wheel into the WebView.
    pub(crate) scrying_rect: Option<(GraphMemberId, [f32; 4])>,
    /// The scrying tile that currently owns the keyboard (clicked into).
    pub(crate) scrying_input_focus: Option<GraphMemberId>,
}

impl WindowView {
    /// Mint a window's view over a fresh pair of serval runners. Everything else
    /// starts at its rest value (empty caches, no in-progress gesture, the default
    /// 1024×600 surface); the caller overrides the view-session bits it restored
    /// (`centered`, `content_location`, `frame_layout`, `next_pane_id`). A second
    /// window is a second `new(...)` over the same shared session. (MW2.)
    pub(crate) fn new(
        kind: WindowKind,
        focused_graph: GraphId,
        dom: Rc<RefCell<ScriptedDom>>,
        runner: ServalAppRunner<Chrome, ChromeLogic, ChromeView>,
        workbench: Workbench,
        workbench_dom: Rc<RefCell<ScriptedDom>>,
        workbench_runner: ServalAppRunner<WorkbenchScene, WorkbenchLogic, WorkbenchTreeView>,
    ) -> Self {
        Self {
            kind,
            focused_graph,
            dom,
            runner,
            chrome_session: None,
            workbench,
            workbench_dom,
            workbench_runner,
            workbench_session: None,
            roster_pane: crate::roster_view::RosterPane::new(),
            session_row_rects: Default::default(),
            session_close_rects: Default::default(),
            session_add_rect: Default::default(),
            apparatus_button_rects: Default::default(),
            gloss_node_rects: Default::default(),
            tile_rects: Default::default(),
            content_rects: Default::default(),
            close_button_rects: Default::default(),
            tile_textures: Default::default(),
            close_button_tex: Default::default(),
            unvisited_tex: Default::default(),
            snapshot_textures: Default::default(),
            window_controls_tex: Default::default(),
            divider_tex: Default::default(),
            scroll: Default::default(),
            roster_scroll: Default::default(),
            last_left_release: Default::default(),
            tab_drag: Default::default(),
            divider_drag: Default::default(),
            frame_divider_drag: Default::default(),
            resize_drag: Default::default(),
            titlebar_press: Default::default(),
            cursor_icon: Default::default(),
            pending_exit: Default::default(),
            context_set: Default::default(),
            renaming: Default::default(),
            centered: Default::default(),
            healed: Default::default(),
            content_location: Default::default(),
            shown_location: Default::default(),
            focused_tile: Default::default(),
            live_previews: Default::default(),
            frame_layout: Default::default(),
            next_pane_id: Default::default(),
            maximized_pane: Default::default(),
            active_content: Default::default(),
            window: Default::default(),
            surface: Default::default(),
            toolbar_h: Default::default(),
            width: 1024,
            height: 600,
            cursor: Default::default(),
            modifiers: Default::default(),
            scrying: Default::default(),
            scrying_rect: Default::default(),
            scrying_input_focus: Default::default(),
        }
    }

    /// Request a redraw of this window, if it exists. A per-window operation: each
    /// window drives its own surface, so the registry's event handlers call this on
    /// the target view directly. (MW2 (c).)
    pub(crate) fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
