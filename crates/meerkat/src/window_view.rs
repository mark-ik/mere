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
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use forme::GraphMemberId;
use frame::{FrameLayout, GraphId, PaneId, SessionId, SplitAxis, SplitChoice};
use meerkat::{Chrome, ChromeView, chrome_view};
use orrery::Face;
use platen::Workbench;
use serval_scripted_dom::ScriptedDom;
use serval_winit_host::WindowSurface;
use winit::window::CursorIcon;
use xilem_serval::{
    AnyView, Modifiers, PointerClick, ServalAppRunner, ServalCtx, ServalElement, WheelEvent, el,
    external_texture, lens, on_click, on_wheel, overlay_rect,
};

use super::{CachedTile, ContentPane, ResizeDrag};
use crate::list_pane::{list_pane_view, ListPaneState, ListView, PaneItem};
use crate::pane_session::PaneSession;
use crate::roster_view::{roster_view, RosterState, RosterView};
use crate::settings_pane_view::{settings_panes_view, SettingsPane, SettingsPanesState, SettingsPanesView};

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

/// An in-progress swatch hull-vertex drag — the node shape editor's edit gesture.
/// The swatch is DOM in the chrome document, so the host hit-tests it and drives the
/// drag from the cursor (serval has no native DOM pointer-drag), mirroring the orrery's
/// node drag. The dragged vertex's new position is written into the node's collider
/// hull on each move, which rebuilds the physics body live. (Swatch — Stage B; the
/// first binding of the general "handle press → drag → mutate the scoped element".)
#[derive(Clone, Copy)]
pub(crate) struct SwatchDrag {
    /// The graph node whose collider hull is being edited (carried on the swatch
    /// container's `data-subject`).
    pub(crate) subject: uuid::Uuid,
    /// Which hull vertex the press grabbed.
    pub(crate) vertex: usize,
    /// The swatch container's painted top-left in window px, so a move maps the cursor
    /// into the swatch's local space.
    pub(crate) origin: (f32, f32),
    /// The swatch's edge length in px, so the local point normalizes to face coords.
    pub(crate) edge: f32,
}

/// An in-progress drag-reorder of a settings-pane list row — the generic row-reorder gesture
/// (a `data-reorder-id` grip drag), whose first consumer is the persona-configurable context
/// menu. `Some` from a press on a row's drag grip until release: the move tracks the drop
/// target, the release repositions + persists. Serval has no native DOM pointer-drag, so the
/// host drives it from the cursor (the swatch editor's pattern). (Command registry B2.)
pub(crate) struct RowReorderDrag {
    /// The grabbed row's `data-reorder-id` (a command id, for the menu list).
    pub(crate) id: String,
    /// The press origin in window px, for the movement threshold.
    pub(crate) origin: (f32, f32),
    /// Whether the pointer has moved past the threshold — a real drag, not a stray grip click.
    pub(crate) moved: bool,
    /// The `data-reorder-id` of the row the cursor is currently over (the drop target), or
    /// `None` when off any reorderable row.
    pub(crate) target: Option<String>,
}

/// Which tear-out operation a drag carries, fixed at press by the modifier (GA-1):
/// Shift = **branch** (a new graphlet in the donor's forme), Ctrl+Shift = **fork** (an
/// independent session + graph snapshot). Leaf (UI-only new window) is the no-modifier
/// tile-tab origin, not an orrery-node tear, so it is not in this enum.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TearOp {
    Branch,
    Fork,
}

/// An in-progress tear-out drag — pulling a node out of its pane toward a new window
/// (the tear-out gesture, G1). Armed on a modified left-press on an orrery node (GA-1:
/// the modifier is what diverts the press from the orrery's node-pin gesture), carried
/// until release. The release decides the operation by where it drops (the drop-target
/// grammar, OQ-1) and how far it moved (slop). v0 tears a single node (GA-4).
#[derive(Clone, Copy)]
pub(crate) struct TearOutDrag {
    /// The torn node's stable id (from the orrery's `node_at_screen` hit-test).
    pub(crate) node: uuid::Uuid,
    /// The graph the torn node lives in (the source pane's graph at press), so the drop
    /// can tell a same-graph tear from a cross-graph copy. (Drop-target grammar, OQ-1.)
    pub(crate) source_graph: GraphId,
    /// The operation fixed at press by the modifier (Shift = branch, Ctrl+Shift = fork).
    pub(crate) op: TearOp,
    /// The press origin in window px, for the slop threshold (a modified click that
    /// never moves is not a tear).
    pub(crate) origin: (f32, f32),
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
    /// Each open settings tile's body rect this frame (member): a left press here routes to
    /// the shell document (the settings pane's spine + controls), not the workbench surface
    /// underneath. Set by the per-frame settings snapshot. (Settings lane P1.)
    pub(crate) settings_rects: Vec<(GraphMemberId, [f32; 4])>,
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
    /// The focused node's "last visit" snapshot preview as a PNG data-URI, keyed by URL.
    /// Built once per url by a blocking readback + encode, then rendered as a chrome `<img>`
    /// in the snapshot card (over the node cards). (Layering fix — card over nodes.)
    pub(crate) snapshot_data_uris: HashMap<String, String>,
    /// Cached rasterized window-control strip (min / max / close).
    pub(crate) window_controls_tex: Option<CachedTile>,
    /// A small solid texture filling the frame-divider gutters between split panes.
    pub(crate) divider_tex: Option<CachedTile>,

    // ── Interaction state: in-progress gestures + scroll + transient view bits
    //    that belong to this window's pointer / keyboard, not the shared session. ─
    /// Per-member content scroll offset (px from the document top). Absent = top.
    /// (The chrome panes — roster / settings body / utility panes — scroll through the
    /// engine's retained `element_scroll` now, driven by `scroll_at`, so they carry no
    /// host-side offset field. This is the per-node *content* document scroll only.)
    pub(crate) scroll: HashMap<GraphMemberId, f32>,
    /// Sticky goal column for soft-wrap ArrowUp/ArrowDown in the knot-editor textarea:
    /// `(caret byte the last vertical move left the caret at, goal x in layout space)`.
    /// Reused only while the caret is still at that byte — an uninterrupted run of
    /// up/down presses — so any edit / click / horizontal move reseeds it. (Soft-wrap
    /// caret nav; the per-buffer hard-line goal lives in `TextInput` instead.)
    pub(crate) soft_wrap_goal: Option<(usize, f32)>,
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
    /// An in-progress swatch hull-vertex drag (the node shape editor). `Some` while a
    /// press that landed on a hull vertex is being dragged; the move reshapes the
    /// collider, the release clears it. (Swatch — Stage B.)
    pub(crate) swatch_drag: Option<SwatchDrag>,
    /// An in-progress drag-reorder of a settings-pane list row (the configurable context menu).
    /// `Some` while a grip press is being dragged; the move tracks the drop target, the release
    /// repositions + persists. (Command registry B2 — drag reorder.)
    pub(crate) row_reorder_drag: Option<RowReorderDrag>,
    /// An in-progress tear-out drag (G1): `Some` from a modified left-press on an orrery
    /// node until release, which spawns a leaf carrying the node. (Tear-out gestures.)
    pub(crate) tear_out_drag: Option<TearOutDrag>,
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
    /// The member whose **object card** is open in the focus slot, if any: a context
    /// action ("Resize") summons a light per-object action card in place of the snapshot
    /// preview, and it stays until Esc / a new selection clears it. P0 carries one widget
    /// (the size-tier stepper). (Object card — P0.)
    pub(crate) object_card: Option<GraphMemberId>,
    /// The field a right-click landed on, stored when the context menu offers "Delete field"
    /// so the drain knows which to retire. (Field regions — delete.)
    pub(crate) context_field: Option<kernel::graph::FieldId>,
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
    /// If this window is a tear-out **branch**, the `GraphletId` it is scoped to (in its
    /// `focused_graph`'s session graphlet index). `None` for the primary + plain leaf
    /// windows (which are the whole-session default graphlet). Phase 1 only *carries* it;
    /// Phase 2 makes the scope visible + accumulates the branch's lineage. (Graphlet
    /// wiring; tear-out gestures G3.)
    pub(crate) branch_graphlet: Option<forme::GraphletId>,
    /// This window's per-pane camera: one [`orrery::Viewport`] per graph it shows in
    /// an Orrery pane. The pooled `Orrery` is the *authority* (graph + physics + node
    /// positions, shared across windows); the **camera/viewport is view state and
    /// lives here**, so two windows on one graph hold distinct viewports over the
    /// shared positions instead of mirroring one. The ctx installs the relevant entry
    /// into its orrery on build and reads it back on drop (see `WindowCtx`); a graph
    /// absent here is seeded from the orrery's current framing the first time this
    /// window shows it. (Camera on the view.)
    pub(crate) viewports: HashMap<GraphId, orrery::Viewport>,

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
    /// The node's graph member id, stamped on the card div as `data-member` so the
    /// orrery a11y projection can pair the laid-out card rect back to its graph node
    /// (the same `data-member` scheme the roster + workbench placeholders use). (Slice 4.)
    pub(crate) member: GraphMemberId,
    pub(crate) label: String,
    pub(crate) x: f32,
    pub(crate) y: f32,
    /// The node's activation-state color (hex), from `Orrery::node_state_color` (no
    /// selection override — selection shows as a ring + lift on the face instead).
    pub(crate) color: String,
    /// Whether the node is selected: the face gets a ring + slight scale-lift, distinct
    /// from the focus ring, so selection leaves the color channel free. (P0 selection.)
    pub(crate) selected: bool,
    /// Whether the cursor is over the node this frame: the face gets a faint brighten
    /// (a wash overlay), the hover channel from Decision 2 — distinct from selection
    /// (ring + lift) and focus (the focusable ring). (P0 hover.)
    pub(crate) hovered: bool,
    /// The node's face footprint (px) before the selection lift, from `Orrery::node_size`
    /// (per-node override / size-by-degree / the uniform default). The card centers the
    /// face on the gyre collider at this size. (P0 resize.)
    pub(crate) size: f32,
    /// The face's `border-radius` for the node's content-type silhouette (square
    /// document / rounded menu `9px` / circle feed `50%`). (P0 shape.)
    pub(crate) radius: &'static str,
    /// The node's favicon as a `data:` image URI, or `None`. It fills the card face
    /// (painted over the state color, clipped to the silhouette). (P0 favicon-as-face.)
    pub(crate) favicon: Option<String>,
    /// The node's custom sprite image as a `data:` image URI, when its face is `Sprite`
    /// (from `Orrery::node_sprite`). Fills the whole face (the imported image), replacing
    /// the state color + favicon. (Node body & face — sprite face.)
    pub(crate) sprite: Option<String>,
    /// The node's **face** (the texture axis), from `Orrery::node_face`: `Favicon` draws the
    /// favicon + caption chip; `Bare` draws the bare content-typed face; `Sprite` draws the
    /// imported image. Independent of the body (the collider shape). (Node body & face.)
    pub(crate) face: Face,
    /// The node's collider hull (face-normalized `[-0.5, 0.5]` polygon), when it has a custom
    /// body. The face is **clipped to it**, so the rendered node matches its collider — a sprite's
    /// transparent background stops reading as a square, and the picture is the body. Empty for a
    /// silhouette-bodied node. (Node body & face — the face is the collider.)
    pub(crate) hull: Vec<(f32, f32)>,
}

/// The focused orrery's render snapshot: the pane rect the element sits at and the
/// node cards (each at its camera-mapped pane-local position). The host rebuilds it
/// each frame so the cards track gyre and the element tracks the pane. (Phase 2.)
#[derive(PartialEq)]
pub(crate) struct OrreryRender {
    /// The orrery pane rect `[x0, y0, x1, y1]` (left, top, right, bottom) in viewport px.
    pub(crate) rect: [f32; 4],
    pub(crate) cards: Vec<OrreryCard>,
    /// The focused node's content card (snapshot or unvisited placeholder), placed
    /// *after* the node cards in document order so it paints over them — the spatial
    /// map's nodes sit under the focused content, and the chrome's overlays still paint
    /// over the card (shell order: orrery before chrome). `None` when no node is focused
    /// or the focused node is itself an open tile. (Layering fix — card over nodes.)
    pub(crate) focus_card: Option<FocusCard>,
}

/// The focused node's content card in the shell document: a positioned element placed
/// after the node cards so document order paints it over them. A `Snapshot` carries a PNG
/// data-URI `<img>` of the page's top peek (built host-side, cached per url); an
/// `Unvisited` is a pure-DOM dashed placeholder. (Layering fix — card over nodes.)
#[derive(PartialEq)]
pub(crate) struct FocusCard {
    /// The card rect `[x0, y0, x1, y1]` local to the orrery element origin.
    pub(crate) rect: [f32; 4],
    pub(crate) kind: FocusCardKind,
}

#[derive(PartialEq)]
pub(crate) enum FocusCardKind {
    /// A visited node's "last visit" preview: the page's top peek rendered to a PNG
    /// data-URI image, placed as a chrome DOM `<img>` after the node cards (so it paints
    /// over them and under the overlays — like the favicons already do). An
    /// external-texture cannot serve here: textures composite in the content layer below
    /// the chrome, and the transparent hole does not erase the opaque node cards behind it.
    /// Only built once the host's once-per-url readback + encode has cached the image, so
    /// there is no empty placeholder while it builds. (Layering fix; no placeholder flash.)
    Snapshot { data_uri: String },
    /// A never-visited node: a static dashed "double-click to load" placeholder.
    Unvisited,
    /// The per-object action card, summoned in place of the preview by a context action: an
    /// ordered list of setting widgets for the selected object's type. Each widget's controls
    /// queue a `node_card_keys` activation the host drains. (Object card — P1.)
    ObjectCard { widgets: Vec<CardWidget> },
}

/// One setting widget on the object card: a control bound to one per-object setting. The
/// node preset (P1) carries the size-tier stepper + the representation toggle; more widgets
/// (engine, color, pin) and per-type presets join as the card generalizes. (Object card — P1.)
#[derive(PartialEq)]
pub(crate) enum CardWidget {
    /// Size-tier stepper rendered as five notch dots filled to `tier`, with − / + buttons
    /// (`size:down` / `size:up`). (Object card — P1.)
    SizeTier { tier: usize },
    /// Face toggle: a Favicon | Plain segmented control, `is_favicon` marking the active one
    /// (`face:favicon` / `face:bare`) — the Face axis. The body (custom hull) is authored in the
    /// shape editor, not this compact toggle. (Object card — P1; node body & face.)
    Face { is_favicon: bool },
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
    pub(crate) panes: [ListPaneState; 5],
    pub(crate) pane_rects: [Option<[f32; 4]>; 5],
    /// The most recent orrery wheel delta (device px), queued by the orrery pane element's
    /// `on_wheel` when the host dispatches a wheel there, and drained by the host into gyre's
    /// pan / Ctrl-zoom. Routes the orrery wheel through the document. (cond 5 input bridge.)
    pub(crate) orrery_wheel: Option<(f32, f32)>,
    /// The open settings tiles, folded into the shell document like the list panes but
    /// variable-length + two-column (index spine + page body), one entry per open
    /// `settings://` node. Empty keeps the subtree out of the document. (Settings lane P1.)
    pub(crate) settings: SettingsPanesState,
    /// Activation keys queued by the object card's widget controls (`size:up` / `size:down`,
    /// `face:favicon` / `face:bare`, …); the host drains + dispatches them for the card's member.
    /// (Object card — P1.)
    pub(crate) node_card_keys: Vec<String>,
}

/// Index into [`ShellState::panes`] / `pane_rects` for the four folded list panes; the
/// order is the array order and [`idx`](Self::idx) is the array index. (Phase 1, step 2.)
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellListPane {
    Apparatus,
    Steward,
    Inspector,
    Trail,
    Alembic,
}

impl ShellListPane {
    pub(crate) fn idx(self) -> usize {
        self as usize
    }
}

/// The positioned-wrapper CSS class for each [`ShellListPane`], by array index — the
/// outer `position:absolute` div that holds the lensed `list_pane_view`. (Phase 1.)
const PANE_WRAPPER_CLASS: [&str; 5] =
    ["apparatus-pane", "steward-pane", "inspector-pane", "trail-pane", "alembic-pane"];

/// Constant-index field accessors into [`ShellState::panes`], one per list pane, so each
/// lensed subtree targets its own slot. Non-capturing, so they coerce to `fn`. (Phase 1.)
const PANE_TO: [fn(&mut ShellState) -> &mut ListPaneState; 5] = [
    |s| &mut s.panes[0],
    |s| &mut s.panes[1],
    |s| &mut s.panes[2],
    |s| &mut s.panes[3],
    |s| &mut s.panes[4],
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
    let list_panes = (list_pane(0), list_pane(1), list_pane(2), list_pane(3), list_pane(4));
    // The settings tiles (variable count), folded in as one lensed subtree that emits one
    // absolutely positioned two-column pane (index spine + page body) per open `settings://`
    // tile. Absent when none are open, so the document is identical before any settings tile.
    // (Settings lane P1.)
    let settings = (!s.settings.panes.is_empty()).then(|| {
        let make: fn(&mut SettingsPanesState) -> SettingsPanesView =
            |s: &mut SettingsPanesState| settings_panes_view(s);
        let to: fn(&mut ShellState) -> &mut SettingsPanesState = |s: &mut ShellState| &mut s.settings;
        Box::new(el::<_, ShellState, ()>("div", lens(make, to)).attr("class", "settings-panes-host"))
            as ShellView
    });
    Box::new(
        el::<_, ShellState, ()>(
            "shell",
            // Document order is paint + hit-test order in the one shell scene: the orrery
            // nodes and the folded panes come first (the content), then the chrome LAST so
            // its modal overlays (context menu, palette, find, settings) paint over the node
            // cards and win the hit-test, instead of the node DOM (formerly later in the
            // document) occluding the menu and stealing its clicks. The toolbar is in normal
            // flow and the content roots are `position:absolute`, so their geometry is
            // unchanged by the reorder; only the z-order is. The settings panes sit with the
            // other folded content (before the chrome), painting over the workbench composite
            // at their tile rects while the chrome overlays still win above them.
            (orrery_element(&s.orrery), roster, list_panes, settings, chrome),
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
    // The face footprint (px): per-node, from `Orrery::node_size` (default 36 = the in-scene
    // gnode + gyre's NODE_HALF collider; size-by-degree / a per-node override raise it). The
    // `.node-card` element IS the face + hit target (not a nested flex item, which serval
    // collapsed to nothing — only the label rendered).
    let face = c.size;
    // Selection lifts the node: its face grows slightly and stays centered on the gyre
    // collider, so the world grab point does not move (Decision 2 — the selection channel is
    // a ring plus a slight lift, leaving the color channel free for activation state). The
    // lift grows width/height and recenters the translate rather than applying a CSS scale,
    // so it needs no transform-origin support; the gyre collider stays `NODE_HALF`, so the
    // lift is visual emphasis only and the hit target is unchanged.
    const LIFT: f32 = 4.0;
    let size = if c.selected { face + LIFT } else { face };
    let half = size / 2.0;
    // Top-left at the node's world position minus half (of the lifted size), the same
    // `pos - NODE_HALF` centering the in-scene gnode uses, so the square stays centered on
    // the node as it lifts.
    let (cx, cy) = (c.x - half, c.y - half);
    // Selection: a bright ring + deeper shadow around the lifted face, distinct from the
    // blue focus ring the `focusable` wrapper draws; else a base depth shadow.
    let ring = if c.selected {
        "box-shadow:0 0 0 2px #ffffff,0 3px 10px rgba(0,0,0,0.6);"
    } else {
        "box-shadow:0 1px 3px rgba(0,0,0,0.45);"
    };
    // The node body: a colored square rendered AT the gyre collider's screen position.
    // `left:0;top:0` anchors it to the orrery element's origin so the transform places it
    // exactly there, not offset by an absolute box's static-flow position (the cause of the
    // collider-vs-visual gap — the press hit the bare collider beside the visual). This IS the
    // node object the collider is pinned to and the drag grabs; the label and (later) the
    // content-preview card anchor to it. Shaped square / rounded / circle by content type.
    // The body shape: a node with a custom hull is **clipped to it**, so the rendered node IS its
    // collider — a sprite's transparent background no longer reads as a square, and the picture
    // matches the physics. (The selection lift still marks selection; a box-shadow ring would be
    // erased by the clip, so it is dropped for a hulled node — a shaped ring is a later refinement.)
    // A silhouette-bodied node keeps the content-type border-radius + the box-shadow ring.
    let shape_style = if c.hull.len() >= 3 {
        let pts: Vec<String> = c
            .hull
            .iter()
            .map(|&(nx, ny)| format!("{:.2}% {:.2}%", (nx + 0.5) * 100.0, (ny + 0.5) * 100.0))
            .collect();
        format!("clip-path:polygon({});", pts.join(", "))
    } else {
        format!("border-radius:{};{ring}", c.radius)
    };
    let face_style = format!(
        "position:absolute;left:0;top:0;transform:translate({cx}px,{cy}px);width:{size}px;\
         height:{size}px;box-sizing:border-box;background-color:{};{shape_style}",
        c.color
    );
    // Face axis: `Bare` is the bare content-typed face (no favicon, no caption), for dense
    // graphs or a node with nothing to texture; `Favicon` (the default) textures the favicon
    // on the face and sets the caption beside it; `Sprite` shows the imported image. The body
    // (collider shape) is a separate axis, so only the face children below differ.
    let chrome = !matches!(c.face, Face::Bare);
    // The face image fills the face (absolutely positioned over the state color, shaped to
    // match): a `Sprite` shows its imported image; a `Favicon` shows the favicon (transparency
    // shows the color through); `Bare` shows neither. (Node body & face — the Face axis.)
    let face_image = match c.face {
        Face::Sprite => c.sprite.as_ref(),
        Face::Favicon => c.favicon.as_ref(),
        Face::Bare => None,
    };
    let favicon = face_image.map(|uri| {
        // A sprite is a photo / artwork: cover-fit fills the face without distortion. A
        // favicon is a small glyph: stretch it to the face as before.
        let fit = if matches!(c.face, Face::Sprite) {
            "object-fit:cover;"
        } else {
            ""
        };
        el::<_, ShellState, ()>("img", ()).attr("src", uri.clone()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{size}px;height:{size}px;border-radius:{};{fit}display:block",
                c.radius
            ),
        )
    });
    // The label rides beside the square (absolutely positioned, overflowing it), like the
    // gnode caption at left:42px, so a long name reads in full on the dark canvas. Absent
    // on a `Shape`, which is the caption-less bare face.
    let label = chrome.then(|| {
        el::<_, ShellState, ()>("span", c.label.clone()).attr(
            "style",
            // Beside the face (gap 6) and vertically centered on it, so the caption tracks
            // the footprint as the node resizes. (P0 resize.)
            format!(
                "position:absolute;left:{}px;top:{}px;white-space:nowrap;color:#d8deea;font-size:14px;font-weight:500",
                size + 6.0,
                (size / 2.0 - 8.0).max(0.0)
            ),
        )
    });
    // Hover (Decision 2): a faint white wash over the face, painted last (after the favicon)
    // so it brightens the whole silhouette. Sized to the face box (left:0..size), so the
    // beside-face label keeps full contrast; `pointer-events:none` keeps it off the grab.
    // (P0 hover.)
    let wash = c.hovered.then(|| {
        el::<_, ShellState, ()>("div", ()).attr(
            "style",
            format!(
                "position:absolute;left:0;top:0;width:{size}px;height:{size}px;border-radius:{};\
                 background-color:rgba(255,255,255,0.16);pointer-events:none",
                c.radius
            ),
        )
    });
    // The card is a static snapshot content-preview, never the node's hit-target or a focus stop
    // (the cond-3/4 reversal: presses route to gyre via CLICK_SLOP, the card is inert). So no
    // `focusable` (it put every on-screen card in the Tab ring ahead of chrome, slice 2) and no
    // `on_click` select (gyre owns selection, slice 3). Keyboard focus is the orrery container, not
    // the per-card sprite. (Slices 2 + 3, 2026-06-21.)
    Box::new(
        el::<_, ShellState, ()>("div", (favicon, label, wash))
            .attr("class", "node-card")
            .attr("data-member", c.member.to_string())
            .attr("style", face_style),
    )
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
    // The focused node's content card paints LAST among the orrery's children, so it sits
    // over the node cards (the spatial map's nodes are under the focused content). The
    // chrome (and its overlays) still paints over it, since the orrery element precedes the
    // chrome in the shell document. (Layering fix — card over nodes.)
    if let Some(fc) = &render.focus_card {
        children.push(focus_card_view(fc));
    }
    // The orrery pane element bears the wheel: a wheel the host dispatches here queues its delta
    // for the host to drain into gyre's pan / Ctrl-zoom, routing the orrery wheel through the
    // document (the form wheel.rs intends). The cards / scene under it have no wheel handler, so
    // the runner's ancestor walk resolves any orrery wheel to this element. (cond 5 input bridge.)
    Box::new(on_wheel(
        el::<_, ShellState, ()>("div", children)
            .attr("class", "orrery")
            .attr(
                "style",
                // z-index:0 makes the orrery the base layer of the shell z-stack: a
                // stacking context that *contains* its node/focus cards, so they paint
                // within it and never hoist to compete with the chrome (z-index:10, above).
                // (Shell z-stack — card under chrome.)
                format!(
                    "position:absolute;left:{x0}px;top:{y0}px;width:{}px;height:{}px;overflow:hidden;z-index:0",
                    x1 - x0,
                    y1 - y0
                ),
            ),
        |s: &mut ShellState, w: WheelEvent| s.orrery_wheel = Some(w.delta),
    ))
}

/// The focused node's content card: a positioned element over the orrery node cards. A
/// `Snapshot` is a framed card holding a PNG data-URI `<img>` of the page's top peek (the
/// host builds + caches it per url); an `Unvisited` is a dashed "double-click to load"
/// placeholder (double-click is host-handled via `content_rects`, so the element needs no
/// click handler). The card is opaque chrome DOM after the node cards, so document order
/// paints it over them and under the chrome overlays. (Layering fix — card over nodes.)
fn focus_card_view(fc: &FocusCard) -> ShellView {
    let [x0, y0, x1, y1] = fc.rect;
    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
    match &fc.kind {
        FocusCardKind::Snapshot { data_uri } => {
            // The preview is a PNG data-URI <img> (like the favicons), so it is opaque chrome
            // DOM after the node cards: document order paints it over them. Only the cached
            // image renders — there is no placeholder while it builds. (Layering fix.)
            let img: ShellView = Box::new(
                el::<_, ShellState, ()>("img", ()).attr("src", data_uri.clone()).attr(
                    "style",
                    "width:100%;height:100%;border-radius:8px;display:block",
                ),
            );
            // `overlay_rect` owns the geometry (and the hit-test class); the card's visuals
            // (clip, radius, shadow) ride the inner div, which fills the positioned box —
            // adding a `style` to the overlay element would clobber its geometry. (Overlay P2.)
            let card: ShellView = Box::new(
                el::<_, ShellState, ()>("div", vec![img]).attr(
                    "style",
                    "width:100%;height:100%;box-sizing:border-box;overflow:hidden;\
                     border-radius:8px;box-shadow:0 6px 24px rgba(0,0,0,0.55)",
                ),
            );
            Box::new(
                overlay_rect::<_, ShellState, ()>(x0, y0, w, h, vec![card])
                    .attr("class", "snapshot-card"),
            )
        }
        FocusCardKind::Unvisited => {
            let inner: ShellView = Box::new(
                el::<_, ShellState, ()>("div", "Double-click to load".to_string()).attr(
                    "style",
                    "width:100%;height:100%;box-sizing:border-box;display:flex;align-items:center;\
                     justify-content:center;border:1px dashed #5a6478;border-radius:8px;\
                     color:#8b94a6;font-size:13px;background:rgba(28,32,40,0.88)",
                ),
            );
            Box::new(
                overlay_rect::<_, ShellState, ()>(x0, y0, w, h, vec![inner])
                    .attr("class", "unvisited-card"),
            )
        }
        // The per-object action card: the widgets' (caption, control) rows flattened as direct
        // children of `object-card`, block-stacked exactly like the P0 single widget did (no
        // flex container, no per-widget wrapper) so the controls' clicks route + fire. The
        // `object-card` class is the click-routing + double-click-suppress gate's key. (P1.)
        FocusCardKind::ObjectCard { widgets } => {
            let mut children: Vec<ShellView> = Vec::with_capacity(widgets.len() * 2);
            for widget in widgets {
                let (caption, control) = object_card_widget_row(widget);
                children.push(caption);
                children.push(control);
            }
            let inner: ShellView = Box::new(
                el::<_, ShellState, ()>("div", children).attr(
                    "style",
                    "width:100%;height:100%;box-sizing:border-box;padding:9px 12px;\
                     border-radius:8px;background:rgba(28,32,40,0.96);\
                     box-shadow:0 6px 24px rgba(0,0,0,0.55)",
                ),
            );
            Box::new(
                overlay_rect::<_, ShellState, ()>(x0, y0, w, h, vec![inner])
                    .attr("class", "object-card"),
            )
        }
    }
}

/// Render one object-card widget as a `(caption, control)` pair the card stacks as direct
/// children. Each control queues a `node_card_keys` activation the host drains + dispatches.
/// (Object card — P1.)
fn object_card_widget_row(widget: &CardWidget) -> (ShellView, ShellView) {
    match widget {
        CardWidget::SizeTier { tier } => {
            let title: ShellView = Box::new(
                el::<_, ShellState, ()>("div", "Size".to_string())
                    .attr("style", "color:#8b94a6;font-size:11px;margin-bottom:5px"),
            );
            let dots: String = (0..orrery::SIZE_TIERS.len())
                .map(|i| if i <= *tier { '\u{25CF}' } else { '\u{25CB}' })
                .collect();
            let btn = "width:30px;height:30px;display:flex;align-items:center;justify-content:center;\
                       border-radius:6px;background:#2a2f3a;color:#d8deea;font-size:20px;font-weight:600;\
                       cursor:pointer;user-select:none";
            let minus: ShellView = Box::new(on_click(
                el::<_, ShellState, ()>("div", "\u{2212}".to_string()).attr("style", btn),
                move |s: &mut ShellState, _: PointerClick| s.node_card_keys.push("size:down".to_string()),
            ));
            let plus: ShellView = Box::new(on_click(
                el::<_, ShellState, ()>("div", "+".to_string()).attr("style", btn),
                move |s: &mut ShellState, _: PointerClick| s.node_card_keys.push("size:up".to_string()),
            ));
            let notches: ShellView = Box::new(
                el::<_, ShellState, ()>("span", dots)
                    .attr("style", "color:#9aa4b8;font-size:15px;letter-spacing:5px"),
            );
            let row: ShellView = Box::new(
                el::<_, ShellState, ()>("div", vec![minus, notches, plus]).attr(
                    "style",
                    "display:flex;align-items:center;justify-content:space-between;margin-bottom:11px",
                ),
            );
            (title, row)
        }
        CardWidget::Face { is_favicon } => {
            let title: ShellView = Box::new(
                el::<_, ShellState, ()>("div", "Face".to_string())
                    .attr("style", "color:#8b94a6;font-size:11px;margin-bottom:5px"),
            );
            let seg = |active: bool| -> String {
                let (bg, fg) = if active { ("#3a4150", "#ffffff") } else { ("#2a2f3a", "#9aa4b8") };
                format!(
                    "flex:1;height:30px;display:flex;align-items:center;justify-content:center;\
                     background:{bg};color:{fg};font-size:13px;cursor:pointer;user-select:none"
                )
            };
            let favicon_btn: ShellView = Box::new(on_click(
                el::<_, ShellState, ()>("div", "Favicon".to_string())
                    .attr("style", format!("{};border-radius:6px 0 0 6px", seg(*is_favicon))),
                move |s: &mut ShellState, _: PointerClick| {
                    s.node_card_keys.push("face:favicon".to_string())
                },
            ));
            let shape_btn: ShellView = Box::new(on_click(
                el::<_, ShellState, ()>("div", "Plain".to_string())
                    .attr("style", format!("{};border-radius:0 6px 6px 0", seg(!*is_favicon))),
                move |s: &mut ShellState, _: PointerClick| {
                    s.node_card_keys.push("face:bare".to_string())
                },
            ));
            let row: ShellView = Box::new(
                el::<_, ShellState, ()>("div", vec![favicon_btn, shape_btn])
                    .attr("style", "display:flex;gap:2px"),
            );
            (title, row)
        }
    }
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
            orrery: OrreryRender { rect: [0.0; 4], cards: Vec::new(), focus_card: None },
            roster: RosterState::default(),
            roster_rect: None,
            panes: std::array::from_fn(|_| ListPaneState::default()),
            pane_rects: [None; 5],
            orrery_wheel: None,
            settings: SettingsPanesState::default(),
            node_card_keys: Vec::new(),
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

    /// Drain the activation keys the object card's widget controls queued, for the host to
    /// dispatch to its member. (Object card — P1.)
    pub(crate) fn take_node_card_keys(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.node_card_keys));
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

    /// Replace the open settings tiles' rendered state (one [`SettingsPane`] per open
    /// `settings://` tile) + the panel background, folding them into the shell document.
    /// Empty `panes` keeps the subtree out. (Settings lane P1.)
    pub(crate) fn set_settings_panes(&mut self, panes: Vec<SettingsPane>, panel_bg: String) {
        self.runner.update(|s| {
            s.settings.panes = panes;
            s.settings.panel_bg = panel_bg;
        });
    }

    /// Whether any settings tile subtree is currently in the shell document, so the host
    /// can skip the per-frame `set_settings_panes` while none are open. (Settings lane P1.)
    pub(crate) fn settings_panes_open(&self) -> bool {
        !self.runner.state().settings.panes.is_empty()
    }

    /// Drain the `(member, key)` page-control activations the settings panes queued, for the
    /// host to apply like an apparatus activation (theme / engine / physics). (Settings lane P1.)
    pub(crate) fn take_settings_pane_keys(&mut self) -> Vec<(GraphMemberId, String)> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.settings.pending_keys));
        out
    }

    /// Drain the `(member, url)` spine navigations the settings panes queued, for the host
    /// to retarget that settings tile's node to the chosen page. (Settings lane P1.)
    pub(crate) fn take_settings_pane_nav(&mut self) -> Vec<(GraphMemberId, String)> {
        let mut out = Vec::new();
        self.runner.update(|s| out = std::mem::take(&mut s.settings.pending_nav));
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
            viewports: HashMap::new(),
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
            settings_rects: Default::default(),
            find_matches: Default::default(),
            find_member: None,
            find_gen: 0,
            tile_textures: Default::default(),
            tile_bands: Default::default(),
            snapshot_data_uris: Default::default(),
            window_controls_tex: Default::default(),
            divider_tex: Default::default(),
            scroll: Default::default(),
            soft_wrap_goal: None,
            last_left_release: Default::default(),
            workbench_gesture: false,
            frame_divider_drag: Default::default(),
            resize_drag: Default::default(),
            titlebar_press: Default::default(),
            swatch_drag: Default::default(),
            row_reorder_drag: Default::default(),
            tear_out_drag: Default::default(),
            branch_graphlet: None,
            cursor_icon: Default::default(),
            pending_exit: Default::default(),
            context_set: Default::default(),
            context_origin: Default::default(),
            context_link: Default::default(),
            object_card: Default::default(),
            context_field: Default::default(),
            renaming: Default::default(),
            tagging: Default::default(),
            centered: Default::default(),
            healed: Default::default(),
            content_location: Default::default(),
            shown_location: Default::default(),
            focused_tile: Default::default(),
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
