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
use meerkat::{Chrome, ChromeView, chrome_view};
use platen::Workbench;
use serval_scripted_dom::ScriptedDom;
use serval_winit_host::WindowSurface;
use winit::window::CursorIcon;
use xilem_serval::{
    AnyView, Modifiers, PointerClick, ServalAppRunner, ServalCtx, ServalElement, WheelEvent, el,
    external_texture, focusable, lens, on_click, on_wheel,
};

use super::{CachedTile, ContentPane, ResizeDrag};
use crate::list_pane::{list_pane_view, ListPaneState, ListView, PaneItem};
use crate::pane_session::PaneSession;
use crate::roster_view::{roster_view, RosterState, RosterView};

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
    pub(crate) runner: ShellRunner,
    /// The chrome DOM's incremental cascade+layout session (cheap-path C3). `None`
    /// until the first render builds it; rebuilt on a structural / resize / theme
    /// change, else the per-frame attribute batch applies on the `RepaintOnly` path.
    pub(crate) chrome_session: Option<PaneSession>,
    /// The tiled-workbench composition (S4): the open tiles + the projection mode
    /// (Cartography = the orrery, Tree = the tiled view).
    pub(crate) workbench: Workbench,
    /// The pelt tile surface the workbench pane renders through (V6): meerkat owns the
    /// `Workbench` (the authority) and projects it onto pelt's tile-tree contract each
    /// frame (`Workbench::to_tile_tree`), drives this GPU-free surface (`set_tree` /
    /// `frame` / `take_events`), and composites each member's actor texture into the
    /// surface's reported tile rects — the same lib pelt's bin wraps. `None` until the
    /// workbench pane first renders.
    /// The host-authoritative tile shell: pelt's pointer state machine over the surface.
    /// The host feeds it pane-local pointer events and drains its [`TileEvent`]s through
    /// `take_events`, applying each to the `Workbench` (the authority) and re-projecting
    /// via `set_tree` — so tab activate/close, tab drag (move/split), and divider resize
    /// all flow through one seam, and the surface stays a driven view. `None` until the
    /// workbench pane first renders.
    pub(crate) pelt_shell: Option<pelt_desktop::TileShell>,
    /// The chrome theme last applied to `pelt_shell` (via `set_theme`), so the tile
    /// theme is rebuilt only when the active theme actually changes, not every frame.
    pub(crate) pelt_theme: Option<register_theme::chrome::ChromeTheme>,
    // The roster is folded into the shell runner now (ShellState.roster); no separate pane.
    // (Unified document host Phase 1.)

    /// Each switcher row's on-screen rect this frame: a click switches to it.
    pub(crate) session_row_rects: Vec<(SessionId, [f32; 4])>,
    /// Each switcher row's close (×) hit rect this frame: a click trashes it.
    pub(crate) session_close_rects: Vec<(SessionId, [f32; 4])>,
    /// The "+" new-graph tile rect this frame, if the switcher is shown.
    pub(crate) session_add_rect: Option<[f32; 4]>,
    /// Each gloss minimap node's rect this frame (node id): a press focuses it.
    pub(crate) gloss_node_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each gloss "recent" row's rect this frame (node id): a press focuses it.
    pub(crate) gloss_recent_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each open tile's content rect this frame (member): the drag resolves its
    /// drop target + zone against it.
    pub(crate) tile_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each composited card/tile's on-screen content rect this frame (member):
    /// routes a wheel over a card to its scroll rather than the orrery.
    pub(crate) content_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Each live card's close-button rect this frame (member): a press reaps that
    /// live preview.
    pub(crate) close_button_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// Find-in-page match rects for `find_member`, computed host-side (no actor
    /// round-trip) against the focused page's cached body — full-document px
    /// (`[x0,y0,x1,y1]`), one inner `Vec` per match. The overlay maps these like the
    /// link rects. Empty when no query / nothing matched. (Find-in-page.)
    pub(crate) find_matches: Vec<Vec<[f32; 4]>>,
    /// The member `find_matches` belongs to (the focused page at query time); the
    /// overlay only draws when it matches the composited member. (Find-in-page.)
    pub(crate) find_member: Option<GraphMemberId>,
    /// The generation of the latest find request shipped to the worker. Replies carry
    /// it back; only the latest is applied (a stale layout for an old query is dropped),
    /// so fast typing never paints an out-of-date highlight set. (Find-in-page.)
    pub(crate) find_gen: u64,

    // ── Paint caches: GPU textures rasterized for this window's surface, reused
    //    across frames while their version + size hold. ────────────────────────
    /// Cached rasterized texture per tile, keyed by member; evicted on close.
    pub(crate) tile_textures: HashMap<GraphMemberId, CachedTile>,
    /// The document-y the cached document-lane texture was rasterized at (the top of
    /// its band). The composite UV-windows within `[band_y, band_y + tex_h]`; absent
    /// (or 0) for HTML-lane / full textures. (Retained-text / tiled render.)
    pub(crate) tile_bands: HashMap<GraphMemberId, f32>,
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
    /// Inspector / steward / apparatus pane scroll offsets (device px), each clamped
    /// to its own laid-out content extent at render. (Same vertical scroll as roster.)
    pub(crate) inspector_scroll: f32,
    pub(crate) steward_scroll: f32,
    pub(crate) apparatus_scroll: f32,
    pub(crate) trail_scroll: f32,
    /// The last left-button release (time + window pos), for double-click detection.
    pub(crate) last_left_release: Option<(Instant, (f32, f32))>,
    /// Whether a left-button gesture inside the workbench pane is in flight (press →
    /// release). While set, pointer moves feed the `pelt_shell`'s pointer state machine
    /// (so a tab drag / divider resize continues even past the pane edge); cleared on
    /// release. Replaces the host-side tab-drag / divider-drag tracking the shell now
    /// owns. (Drag via pelt TileEvents.)
    pub(crate) workbench_gesture: bool,
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
    /// The content-band cursor point (orrery-leaf-local px) an empty-space context
    /// menu was opened at, so `AddNode` mints the node under the cursor. Set in
    /// `open_context_menu_at`, taken in `drain_pending_context`.
    pub(crate) context_origin: Option<(f32, f32)>,
    /// The link a right-click-on-link context menu acts on: `(source member, resolved
    /// url)`. Set when the menu opens over a tile / card link; consumed by the
    /// open-in-new-tab / copy-link actions. (Browser link flow.)
    pub(crate) context_link: Option<(GraphMemberId, String)>,
    /// In-progress session rename: the target session + its edit buffer. `Some` while
    /// the switcher label is being typed (F2 / right-click a tile).
    pub(crate) renaming: Option<(SessionId, String)>,
    /// In-progress node tag entry: the live buffer. `Some` while the host is
    /// capturing a tag for the selected node(s) (context menu → "Add tag…"); the
    /// committed text inserts as a tag on the orrery selection. (Add-tag.)
    pub(crate) tagging: Option<String>,

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
    /// Live workbench-mirror mode: when set, the focused orrery re-scopes to the
    /// workbench's open tiles every frame, so the spatial map tracks the tile set as
    /// it changes. Off by default. (Curated orrery — workbench mirror.)
    pub(crate) mirror_tiles: bool,
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
    /// Every live scrying surface's (member, window rect) this frame, set by render;
    /// the input path hit-tests the list to forward mouse / wheel into the pane under
    /// the cursor. Several compat tiles can be live at once. (Multi-tile scry.)
    pub(crate) scrying_rects: Vec<(GraphMemberId, [f32; 4])>,
    /// The scrying tile that currently owns the keyboard (clicked into).
    pub(crate) scrying_input_focus: Option<GraphMemberId>,
}

/// A single orrery node card in the shell document: a label placed by a per-node
/// transform (gyre's world position). The host snapshots the focused orrery into a
/// `Vec<OrreryCard>` each frame, and the orrery element renders one card per entry.
/// (Orrery-as-element — Phase 2.)
#[derive(PartialEq)]
pub(crate) struct OrreryCard {
    pub(crate) label: String,
    /// The node's URL, so a card click selects it (the two-hit-test's DOM half). (Phase 2.)
    pub(crate) url: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The node's activation-state color (hex), from `Orrery::node_state_color` (no
    /// selection override — selection shows as a ring + lift on the face instead).
    pub(crate) color: String,
    /// Whether the node is selected: the face gets a ring + slight scale-lift, distinct
    /// from the focus ring, so selection leaves the color channel free. (P0 selection.)
    pub(crate) selected: bool,
    /// The face's `border-radius` for the node's content-type silhouette (square
    /// document / rounded menu `9px` / circle feed `50%`). (P0 shape.)
    pub(crate) radius: &'static str,
    /// The node's favicon as a `data:` image URI, or `None`. It fills the card face
    /// (painted over the state color, clipped to the silhouette). (P0 favicon-as-face.)
    pub(crate) favicon: Option<String>,
}

/// The focused orrery's render snapshot: the pane rect the element sits at and the
/// node cards (each at its camera-mapped pane-local position). The host rebuilds it
/// each frame so the cards track gyre and the element tracks the pane. (Phase 2.)
#[derive(PartialEq)]
pub(crate) struct OrreryRender {
    /// The orrery pane rect `[x0, y0, x1, y1]` (left, top, right, bottom) in viewport px.
    pub(crate) rect: [f32; 4],
    pub(crate) cards: Vec<OrreryCard>,
}

/// The window shell's composed view-state: the chrome plus the orrery-as-element's
/// render snapshot (and later the document panes), all one document under one runner.
/// (Unified document host.)
pub(crate) struct ShellState {
    pub(crate) chrome: Chrome,
    /// The focused orrery's render snapshot (rect + cards), refreshed each frame. Empty
    /// until the host wires the snapshot. (Orrery-as-element — Phase 2.)
    pub(crate) orrery: OrreryRender,
    /// The roster pane's view state, folded into the shell document so its rows render,
    /// hit-test, and project a11y through the one shell runner. (Phase 1, pane consolidation.)
    pub(crate) roster: RosterState,
    /// The roster's window rect `[x0,y0,x1,y1]`, `Some` while the pane is open; the shell
    /// view positions the roster subtree there. `None` keeps it out of the document.
    pub(crate) roster_rect: Option<[f32; 4]>,
    /// The four generic list panes (apparatus, steward, inspector, trail), folded into the
    /// shell document like the roster: indexed by [`ShellListPane`], each rendered as a
    /// lensed `list_pane_view` subtree when its matching rect is `Some`. (Phase 1, step 2.)
    pub(crate) panes: [ListPaneState; 4],
    pub(crate) pane_rects: [Option<[f32; 4]>; 4],
    /// URLs queued by orrery card clicks (the two-hit-test's DOM half): a card press dispatches
    /// here through the shell hit-test, and the host drains + selects them. (Phase 2.)
    pub(crate) orrery_card_selects: Vec<String>,
    /// The most recent orrery wheel delta (device px), queued by the orrery pane element's
    /// `on_wheel` when the host dispatches a wheel there, and drained by the host into gyre's
    /// pan / Ctrl-zoom. Routes the orrery wheel through the document. (cond 5 input bridge.)
    pub(crate) orrery_wheel: Option<(f32, f32)>,
}

/// Index into [`ShellState::panes`] / `pane_rects` for the four folded list panes; the
/// order is the array order and [`idx`](Self::idx) is the array index. (Phase 1, step 2.)
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellListPane {
    Apparatus,
    Steward,
    Inspector,
    Trail,
}

impl ShellListPane {
    pub(crate) fn idx(self) -> usize {
        self as usize
    }
}

/// The positioned-wrapper CSS class for each [`ShellListPane`], by array index — the
/// outer `position:absolute` div that holds the lensed `list_pane_view`. (Phase 1.)
const PANE_WRAPPER_CLASS: [&str; 4] =
    ["apparatus-pane", "steward-pane", "inspector-pane", "trail-pane"];

/// Constant-index field accessors into [`ShellState::panes`], one per list pane, so each
/// lensed subtree targets its own slot. Non-capturing, so they coerce to `fn`. (Phase 1.)
const PANE_TO: [fn(&mut ShellState) -> &mut ListPaneState; 4] = [
    |s| &mut s.panes[0],
    |s| &mut s.panes[1],
    |s| &mut s.panes[2],
    |s| &mut s.panes[3],
];

/// The erased shell root view, like [`ChromeView`] but over [`ShellState`].
pub(crate) type ShellView = Box<dyn AnyView<ShellState, (), ServalCtx, ServalElement>>;

/// The runner logic: shell state to shell view.
pub(crate) type ShellLogic = fn(&ShellState) -> ShellView;

/// The runner type the window holds: one document over the composed shell state.
pub(crate) type ShellRunner = ServalAppRunner<ShellState, ShellLogic, ShellView>;

/// The shell root view: a full-bleed container holding the chrome (lifted onto
/// `ShellState` via `lens`) and the orrery element as siblings. The chrome stays in
/// normal flow exactly as it laid out when it was the root; the orrery element is
/// absolutely positioned, so it does not disturb the chrome. (Orrery-as-element.)
fn shell_view(s: &ShellState) -> ShellView {
    let make_chrome: fn(&mut Chrome) -> ChromeView = |c: &mut Chrome| chrome_view(c);
    let to_chrome: fn(&mut ShellState) -> &mut Chrome = |s: &mut ShellState| &mut s.chrome;
    let chrome = lens(make_chrome, to_chrome);
    // The roster pane, when open, is a positioned subtree of the shell document: its view
    // is lensed onto `ShellState.roster`, so the one shell runner renders it, hit-tests it,
    // and dispatches its row clicks. `None` keeps the document identical to before the fold.
    // (Phase 1.)
    let roster = s.roster_rect.map(|[x0, y0, x1, y1]| {
        let make_roster: fn(&mut RosterState) -> RosterView = |r: &mut RosterState| roster_view(r);
        let to_roster: fn(&mut ShellState) -> &mut RosterState = |s: &mut ShellState| &mut s.roster;
        Box::new(
            el::<_, ShellState, ()>("div", lens(make_roster, to_roster))
                .attr("class", "roster-pane")
                .attr(
                    "style",
                    format!(
                        "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                        x1 - x0,
                        y1 - y0
                    ),
                ),
        ) as ShellView
    });
    // The four list panes (apparatus / steward / inspector / trail), each a positioned
    // subtree of the shell document when open: its inner `list_pane_view` is lensed onto
    // the matching `panes` slot, so the one shell runner lays it out, scrolls it, and
    // dispatches its button clicks. `None` rects keep the document unchanged. (Phase 1.)
    let list_pane = |i: usize| -> Option<ShellView> {
        s.pane_rects[i].map(|[x0, y0, x1, y1]| {
            let make: fn(&mut ListPaneState) -> ListView = |st| list_pane_view(st);
            Box::new(
                el::<_, ShellState, ()>("div", lens(make, PANE_TO[i]))
                    .attr("class", PANE_WRAPPER_CLASS[i])
                    .attr(
                        "style",
                        format!(
                            "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                            x1 - x0,
                            y1 - y0
                        ),
                    ),
            ) as ShellView
        })
    };
    let list_panes = (list_pane(0), list_pane(1), list_pane(2), list_pane(3));
    Box::new(
        el::<_, ShellState, ()>(
            "shell",
            // Document order is paint + hit-test order in the one shell scene: the orrery
            // nodes and the folded panes come first (the content), then the chrome LAST so
            // its modal overlays (context menu, palette, find, settings) paint over the node
            // cards and win the hit-test, instead of the node DOM (formerly later in the
            // document) occluding the menu and stealing its clicks. The toolbar is in normal
            // flow and the content roots are `position:absolute`, so their geometry is
            // unchanged by the reorder; only the z-order is.
            (orrery_element(&s.orrery), roster, list_panes, chrome),
        )
        .attr("style", "position:relative;width:100%;height:100%"),
    )
}

/// The external-texture key for the orrery scene underlay: a reserved high value, disjoint from the
/// workbench's per-member UUID-low-64 keys. The host rasterizes the gyre scene under it and
/// composites it at the element's laid-out rect, which `render.rs` enumerates from the document
/// (the external-texture-element compose). (cond 5.)
pub(crate) const ORRERY_SCENE_KEY: u64 = 0xF0F0_0000_0000_0001;

/// One orrery node card: a fixed-footprint square "face" carrying the node's
/// activation-state color (and favicon, when present) shaped by content type, with the
/// label beside it. Selection rings + lifts the face, distinct from the focus ring the
/// `focusable` wrapper draws; the card click-selects its node through the shell hit-test
/// (the two-hit-test's DOM half). The object anatomy mirrors the in-scene gnode the
/// secondary panes still draw. (Node representation P0.)
fn node_card_view(c: &OrreryCard) -> ShellView {
    // The face footprint (px): a fixed square, matching the in-scene gnode + gyre's
    // NODE_HALF collider. The `.node-card` element IS the face + hit target (not a nested
    // flex item, which serval collapsed to nothing — only the label rendered).
    const FACE: f32 = 36.0;
    let half = FACE / 2.0;
    // Top-left at the node's world position minus half, the same `pos - NODE_HALF` the
    // in-scene gnode uses, so the square centers on the node.
    let (cx, cy) = (c.x - half, c.y - half);
    // Selection: a bright ring around the face, distinct from the blue focus ring; else a
    // base depth shadow. (A slight scale-lift is a follow-up, once a composite
    // translate+scale transform is confirmed to render.)
    let ring = if c.selected {
        "box-shadow:0 0 0 2px #ffffff,0 2px 8px rgba(0,0,0,0.55);"
    } else {
        "box-shadow:0 1px 3px rgba(0,0,0,0.45);"
    };
    // The node body: a fixed colored square rendered AT the gyre collider's screen position.
    // `left:0;top:0` anchors it to the orrery element's origin so the transform places it
    // exactly there, not offset by an absolute box's static-flow position (the cause of the
    // collider-vs-visual gap — the press hit the bare collider beside the visual). This IS the
    // node object the collider is pinned to and the drag grabs; the label and (later) the
    // content-preview card anchor to it. Shaped square / rounded / circle by content type.
    let face_style = format!(
        "position:absolute;left:0;top:0;transform:translate({cx}px,{cy}px);width:{FACE}px;\
         height:{FACE}px;box-sizing:border-box;background-color:{};border-radius:{};{ring}",
        c.color, c.radius
    );
    // The favicon fills the face (absolutely positioned over the state color, shaped to
    // match); transparency shows the color through.
    let favicon = c.favicon.as_ref().map(|uri| {
        el::<_, ShellState, ()>("img", ()).attr("src", uri.clone()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{FACE}px;height:{FACE}px;border-radius:{};display:block",
                c.radius
            ),
        )
    });
    // The label rides beside the square (absolutely positioned, overflowing it), like the
    // gnode caption at left:42px, so a long name reads in full on the dark canvas.
    let label = el::<_, ShellState, ()>("span", c.label.clone()).attr(
        "style",
        "position:absolute;left:42px;top:9px;white-space:nowrap;color:#d8deea;font-size:14px;font-weight:500",
    );
    let url = c.url.clone();
    Box::new(focusable(on_click(
        el::<_, ShellState, ()>("div", (favicon, label))
            .attr("class", "node-card")
            .attr("style", face_style),
        move |s: &mut ShellState, _: PointerClick| s.orrery_card_selects.push(url.clone()),
    )))
}

/// The orrery element: a positioned container whose node cards are `position:absolute`
/// + `transform: translate(...)` DOM placed by gyre's world positions (the cards both
/// paint and hit-test where the transform puts them). Empty until the host snapshots
/// the focused orrery; the underlay (edges + demoted dots) joins in (ii). The rect is a
/// placeholder until the frame tree drives the container layout (iii). (Phase 2.)
fn orrery_element(render: &OrreryRender) -> ShellView {
    let card_views: Vec<ShellView> = render.cards.iter().map(node_card_view).collect();
    let [x0, y0, x1, y1] = render.rect;
    let (pw, ph) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    // The orrery scene (gyre edges / backdrop / demoted dots), which the host rasterizes to a
    // texture, sits as an `<external-texture>` underlay in the document so its placement comes from
    // layout and the cards stack over it via the DOM. First child = painted first = under the
    // cards. (cond 5: the scene becomes a document element, not a standalone host composite.)
    let scene: ShellView = Box::new(
        external_texture::<ShellState, ()>(ORRERY_SCENE_KEY, pw as u32, ph as u32)
            .attr("class", "orrery-scene")
            .attr("style", format!("position:absolute;left:0;top:0;width:{pw}px;height:{ph}px")),
    );
    let mut children: Vec<ShellView> = vec![scene];
    children.extend(card_views);
    // The orrery pane element bears the wheel: a wheel the host dispatches here queues its delta
    // for the host to drain into gyre's pan / Ctrl-zoom, routing the orrery wheel through the
    // document (the form wheel.rs intends). The cards / scene under it have no wheel handler, so
    // the runner's ancestor walk resolves any orrery wheel to this element. (cond 5 input bridge.)
    Box::new(on_wheel(
        el::<_, ShellState, ()>("div", children)
            .attr("class", "orrery")
            .attr(
                "style",
                format!(
                    "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden",
                    x1 - x0,
                    y1 - y0
                ),
            ),
        |s: &mut ShellState, w: WheelEvent| s.orrery_wheel = Some(w.delta),
    ))
}

/// Build a window's shell runner over `dom`, seeded with `chrome`. The host view
/// constructor and `main`'s window builders use this instead of a bare chrome runner.
/// (Unified document host — Phase 1.)
pub(crate) fn shell_runner(dom: Rc<RefCell<ScriptedDom>>, chrome: Chrome) -> ShellRunner {
    ServalAppRunner::new(
        dom,
        shell_view as ShellLogic,
        ShellState {
            chrome,
            orrery: OrreryRender { rect: [0.0; 4], cards: Vec::new() },
            roster: RosterState::default(),
            roster_rect: None,
            panes: std::array::from_fn(|_| ListPaneState::default()),
            pane_rects: [None; 4],
            orrery_card_selects: Vec::new(),
            orrery_wheel: None,
        },
    )
}

impl WindowView {
    /// The chrome view-state, read-only. Phase 1 insulation seam: call sites read the
    /// chrome through this rather than `runner.state()` directly, so the runner's state
    /// type can later widen from `Chrome` to a composed shell state without touching
    /// them. (Unified document host — Phase 1.)
    pub(crate) fn chrome(&self) -> &Chrome {
        &self.runner.state().chrome
    }

    /// Mutate the chrome view-state; the runner re-renders and diffs the change. The
    /// mutation counterpart to [`chrome`](Self::chrome), and the same insulation seam.
    pub(crate) fn chrome_update(&mut self, f: impl FnOnce(&mut Chrome)) {
        self.runner.update(|s| f(&mut s.chrome));
    }

    /// Replace the orrery element's node cards with a fresh gyre snapshot. The runner
    /// re-renders, re-placing the cards by their transforms on the RepaintOnly path.
    /// (Orrery-as-element — Phase 2.)
    pub(crate) fn set_orrery(&mut self, render: OrreryRender) {
        self.runner.update(|s| s.orrery = render);
    }

    /// The orrery render snapshot currently in the shell state, so the host can skip
    /// the rebuild when a fresh snapshot is identical (a settled orrery costs no
    /// per-frame view re-run). (Orrery-as-element — Phase 2.)
    pub(crate) fn orrery_render(&self) -> &OrreryRender {
        &self.runner.state().orrery
    }

    /// Drain the URLs the orrery card click handlers queued through the shell runner's
    /// dispatch (the two-hit-test's DOM half), for the host to select. (Phase 2.)
    pub(crate) fn take_orrery_card_selects(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.orrery_card_selects));
        out
    }

    /// Drain the most recent orrery wheel delta the pane element's `on_wheel` queued (device px),
    /// for the host to apply to gyre's pan / Ctrl-zoom. (cond 5 input bridge.)
    pub(crate) fn take_orrery_wheel(&mut self) -> Option<(f32, f32)> {
        let mut out = None;
        self.runner.update(|s| out = s.orrery_wheel.take());
        out
    }

    /// Fold the roster pane into the shell document: replace its rows + window rect (or
    /// `None` to close it). The runner re-renders, diffing the roster subtree into the one
    /// shell DOM, so it lays out, hit-tests, and projects a11y with the chrome. (Phase 1.)
    pub(crate) fn set_roster(
        &mut self,
        rows: Vec<crate::roster::RosterRow>,
        field_rows: Vec<crate::roster::FieldRow>,
        rect: Option<[f32; 4]>,
    ) {
        self.runner.update(|s| {
            s.roster.rows = rows;
            s.roster.field_rows = field_rows;
            s.roster_rect = rect;
        });
    }

    /// Whether the roster subtree is currently in the shell document (the pane is open),
    /// so the host can skip the per-frame `set_roster` while it stays closed. (Phase 1.)
    pub(crate) fn roster_open(&self) -> bool {
        self.runner.state().roster_rect.is_some()
    }

    /// Drain the selections / field intents the roster's row handlers queued through the
    /// shell runner's dispatch, for the host to apply. (Phase 1.)
    pub(crate) fn take_roster_intents(&mut self) -> Vec<crate::roster_view::RosterIntent> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.roster.pending));
        out
    }

    /// Fold a list pane (apparatus / steward / inspector / trail) into the shell document:
    /// set its root class + items + window rect (or `None` to close it). The runner
    /// re-renders, diffing the pane subtree into the one shell DOM so it lays out, scrolls,
    /// and dispatches button clicks with the chrome. (Phase 1, step 2.)
    pub(crate) fn set_list_pane(
        &mut self,
        which: ShellListPane,
        root_class: &str,
        items: Vec<PaneItem>,
        rect: Option<[f32; 4]>,
    ) {
        let i = which.idx();
        let root_class = root_class.to_string();
        self.runner.update(|s| {
            s.panes[i].root_class = root_class;
            s.panes[i].items = items;
            s.pane_rects[i] = rect;
        });
    }

    /// Whether a list pane's subtree is currently in the shell document (the pane is open),
    /// so the host can skip the per-frame `set_list_pane` while it stays closed. (Phase 1.)
    pub(crate) fn list_pane_open(&self, which: ShellListPane) -> bool {
        self.runner.state().pane_rects[which.idx()].is_some()
    }

    /// Drain the activation keys a list pane's button handlers queued through the shell
    /// runner's dispatch, for the host to apply (theme / engine / physics / recover).
    /// (Phase 1, step 2.)
    pub(crate) fn take_list_pane_activations(&mut self, which: ShellListPane) -> Vec<String> {
        let i = which.idx();
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.panes[i].pending));
        out
    }

    /// Mint a window's view over a fresh pair of serval runners. Everything else
    /// starts at its rest value (empty caches, no in-progress gesture, the default
    /// 1024×600 surface); the caller overrides the view-session bits it restored
    /// (`centered`, `content_location`, `frame_layout`, `next_pane_id`). A second
    /// window is a second `new(...)` over the same shared session. (MW2.)
    pub(crate) fn new(
        kind: WindowKind,
        focused_graph: GraphId,
        dom: Rc<RefCell<ScriptedDom>>,
        runner: ShellRunner,
        workbench: Workbench,
    ) -> Self {
        Self {
            kind,
            focused_graph,
            dom,
            runner,
            chrome_session: None,
            workbench,
            pelt_shell: None,
            pelt_theme: None,
            session_row_rects: Default::default(),
            session_close_rects: Default::default(),
            session_add_rect: Default::default(),
            gloss_node_rects: Default::default(),
            gloss_recent_rects: Default::default(),
            tile_rects: Default::default(),
            content_rects: Default::default(),
            close_button_rects: Default::default(),
            find_matches: Default::default(),
            find_member: None,
            find_gen: 0,
            tile_textures: Default::default(),
            tile_bands: Default::default(),
            close_button_tex: Default::default(),
            unvisited_tex: Default::default(),
            snapshot_textures: Default::default(),
            window_controls_tex: Default::default(),
            divider_tex: Default::default(),
            scroll: Default::default(),
            roster_scroll: Default::default(),
            inspector_scroll: Default::default(),
            steward_scroll: Default::default(),
            apparatus_scroll: Default::default(),
            trail_scroll: Default::default(),
            last_left_release: Default::default(),
            workbench_gesture: false,
            frame_divider_drag: Default::default(),
            resize_drag: Default::default(),
            titlebar_press: Default::default(),
            cursor_icon: Default::default(),
            pending_exit: Default::default(),
            context_set: Default::default(),
            context_origin: Default::default(),
            context_link: Default::default(),
            renaming: Default::default(),
            tagging: Default::default(),
            centered: Default::default(),
            healed: Default::default(),
            content_location: Default::default(),
            shown_location: Default::default(),
            focused_tile: Default::default(),
            live_previews: Default::default(),
            frame_layout: Default::default(),
            next_pane_id: Default::default(),
            mirror_tiles: false,
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
            scrying_rects: Default::default(),
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
