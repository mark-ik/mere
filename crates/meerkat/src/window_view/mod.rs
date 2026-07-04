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

use armillary::Generations;
use forme::GraphMemberId;
use frame::{FrameLayout, GraphId, PaneId, SessionId, SplitAxis, SplitChoice};
use meerkat::{Chrome, ChromeView, chrome_view};
use platen::Workbench;
use serval_scripted_dom::{NodeId, ScriptedDom};
use serval_winit_host::WindowSurface;
use session_runtime::{StartupUnlockMode, settings_store::ScriptPermissionPrefs};
use winit::window::CursorIcon;
use xilem_serval::{
    AnyView, Modifiers, PointerClick, ServalAppRunner, ServalCtx, ServalElement, WheelEvent, el,
    external_texture, host_pool, lens, on_click, on_wheel, overlay_rect,
};

use super::{CachedTile, ContentPane, ResizeDrag};
use crate::gloss_outline_view::{GlossOutlineState, GlossOutlineView, gloss_outline_view};
use crate::gloss_view::{
    GlossMinimapState, GlossMinimapView, GlossRecentState, GlossRecentView, minimap_view,
    recent_view,
};
use crate::list_pane::{ListPaneState, ListView, PaneItem, list_pane_view};
use crate::pane_session::PaneSession;
use crate::roster_view::{RosterState, RosterView, roster_view};
use crate::settings_pane_view::{
    SettingsPane, SettingsPanesState, SettingsPanesView, settings_panes_view,
};

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

/// A live page-text selection on a content tile/card. The source page is identified
/// by `(member, gens)` so navigation/resize invalidate it while scroll-band
/// re-emits do not.
pub(crate) struct PageTextSelection {
    pub(crate) member: GraphMemberId,
    pub(crate) gens: Generations,
    pub(crate) rects: Vec<[f32; 4]>,
    pub(crate) text: String,
}

#[derive(Clone, Copy)]
pub(crate) enum PageTextAnchor {
    Document { source_index: usize },
    Html { point: (f32, f32) },
}

/// An in-progress drag selection over a content tile/card. The anchor is either a
/// retained document source block or an HTML content-local point; it stays tied to
/// the same `(member, gens)` until release.
pub(crate) struct PageTextDrag {
    pub(crate) member: GraphMemberId,
    pub(crate) gens: Generations,
    pub(crate) anchor: PageTextAnchor,
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
    /// This window's retained host-owned `.gnode` island under the shell document's
    /// orrery element. Bound to one graph at a time, over the shared pooled orrery.
    pub(crate) gnode_pool: GnodePool,
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
    /// The UI scale last applied to `pelt_shell`, so the tile theme and drag ghost are
    /// only rebuilt when the chrome's effective scale changes.
    pub(crate) pelt_ui_scale: Option<f32>,
    // The roster is folded into the shell runner now (ShellState.roster); no separate pane.
    // (Unified document host Phase 1.)
    /// Each switcher row's on-screen rect this frame: a click switches to it.
    pub(crate) session_row_rects: Vec<(SessionId, [f32; 4])>,
    /// Each switcher row's close (×) hit rect this frame: a click trashes it.
    pub(crate) session_close_rects: Vec<(SessionId, [f32; 4])>,
    /// The "+" new-graph tile rect this frame, if the switcher is shown.
    pub(crate) session_add_rect: Option<[f32; 4]>,
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
    /// The `pelt/scripts` page's capability opinion, cached while that page is open so its
    /// per-frame rebuild reads it in memory instead of re-parsing `settings.json` each frame.
    /// `Some` only while a scripts tile is open: loaded from disk when the page opens, updated
    /// in place on an edit, cleared when it closes (so a reopen re-reads, picking up any
    /// out-of-band change). The resolved bindings the page also lists come from the
    /// constellation's in-memory set. (Settings perf — no per-frame disk read.)
    pub(crate) script_caps: Option<ScriptPermissionPrefs>,
    /// The `pelt/wallet` page's cached startup-unlock mode, loaded when that page opens so its
    /// per-frame rebuild stays off disk, updated in place on edit, and cleared on close so a
    /// reopen re-reads the persisted value. (Startup unlock flow.)
    pub(crate) wallet_unlock_mode: Option<StartupUnlockMode>,
    /// Whether device-local sealed wallet secrets are still locked this launch. Loaded when the
    /// wallet settings page opens, updated in place on explicit unlock, and cleared on close so
    /// a reopen re-reads the current runtime state. (Startup unlock flow.)
    pub(crate) wallet_locked: Option<bool>,
    /// One-line status from the wallet settings page's explicit unlock action. Kept only while
    /// that page stays open so the next reopen starts from live state again. (Startup unlock flow.)
    pub(crate) wallet_unlock_status: Option<String>,
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
    /// The current page-text selection on a retained document-lane content card, if
    /// any. The overlay paints these rects and `Ctrl/Cmd+C` copies `text`. Cleared by
    /// a new selection or when its page version changes. (Retained-text P4.)
    pub(crate) page_selection: Option<PageTextSelection>,

    // ── Paint caches: GPU textures rasterized for this window's surface, reused
    //    across frames while their version + size hold. ────────────────────────
    /// Cached rasterized texture per tile, keyed by member; evicted on close.
    pub(crate) tile_textures: HashMap<GraphMemberId, CachedTile>,
    /// The document-y the cached document-lane texture was rasterized at (the top of
    /// its band). The composite UV-windows within `[band_y, band_y + tex_h]`; absent
    /// (or 0) for HTML-lane / full textures. (Retained-text / tiled render.)
    pub(crate) tile_bands: HashMap<GraphMemberId, f32>,
    /// Full laid-out heights for note tiles rendered host-side through
    /// `note_surface`. The content actor owns this for web/document lanes; local
    /// notes have no actor, so the window caches the measured height from the
    /// latest note band render.
    pub(crate) note_content_heights: HashMap<GraphMemberId, u32>,
    /// The focused node's "last visit" snapshot preview as a PNG data-URI, keyed by member.
    /// Each entry carries the URL it was built for, so an in-place navigation invalidates the
    /// cached image while two nodes on the same URL can still carry different previews. Built
    /// either from the node's persisted thumbnail PNG or by a blocking synthetic readback, then
    /// rendered as a chrome `<img>` in the snapshot card (over the gnodes). (Layering fix.)
    pub(crate) snapshot_data_uris: HashMap<GraphMemberId, SnapshotDataUri>,
    /// Cached rasterized chrome **base** texture (the shell document with the `.orrery`
    /// subtree omitted). Reused across frames while only the orrery subtree churns.
    pub(crate) chrome_base_tex: Option<CachedTile>,
    /// Cached rasterized `.orrery` DOM subtree (gnodes + focus card, but not the
    /// external-texture underlay). Reused when the shell stays settled.
    pub(crate) chrome_orrery_tex: Option<CachedTile>,
    /// Signature of the stylesheet set the cached chrome base texture was rendered from.
    /// A sheet or scale change invalidates the cache even if the DOM mutation stream is
    /// empty for that frame.
    pub(crate) chrome_base_sig: u64,
    /// Last shellbar geometry style the host stamped into the shell DOM. Used to suppress
    /// identical render-patched writes so the shell base can stay clean on settled frames.
    pub(crate) shellbar_style: Option<String>,
    /// Last comms-pane geometry style the host stamped into the shell DOM. Cleared when the
    /// pane leaves the document so a later reopen re-stamps it even if the rect matches.
    pub(crate) comms_style: Option<String>,
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
    /// An in-progress caret drag-select: the text-input DOM node a left press placed the
    /// caret in. While `Some`, each pointer move extends that field's selection to the
    /// byte under the cursor; the release disarms it. Armed only by the press path (a
    /// release-resolved toolbar click never arms — the button is already up). (Djot
    /// editor — drag-select.)
    pub(crate) caret_drag: Option<NodeId>,
    /// An in-progress drag-select over a retained document-lane card. Pointer moves
    /// update the selection; release disarms it. (Retained-text P4.)
    pub(crate) page_text_drag: Option<PageTextDrag>,
    /// An in-progress tear-out drag (G1): `Some` from a modified left-press on an orrery
    /// node until release, which spawns a leaf carrying the node. (Tear-out gestures.)
    pub(crate) tear_out_drag: Option<TearOutDrag>,
    /// An armed web-clip picker: the surface member whose next left press should capture
    /// an element instead of forwarding the click into the surface.
    pub(crate) clip_picker: Option<GraphMemberId>,
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
    /// This window's per-pane node **selection** (and thus focus), one member-uuid set
    /// per graph it shows in an Orrery pane. Like `viewports`, the pooled `Orrery` holds
    /// the live slot and the per-window state lives here: the ctx installs this window's
    /// selection into the orrery on build and reads it back on drop, so two windows on
    /// one graph select / focus independently over the shared positions (and a branch
    /// window's lineage records *its own* focus, not the donor's). Member-keyed so it
    /// survives an evict+reload. A graph absent here adopts the orrery's current
    /// selection the first time this window shows it. (Per-window focus isolation.)
    pub(crate) selections: HashMap<GraphId, Vec<uuid::Uuid>>,

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
    /// This window's display device-pixel-ratio (its monitor's winit `scale_factor`),
    /// set at window creation + on `ScaleFactorChanged`. Per-window so two windows on
    /// monitors of different density each scale correctly; `user_zoom` stays shared on
    /// `Presentation`. The shared chrome sheet is rebuilt to this window's dpi at its
    /// render when it differs from the sheet's current bake. 1.0 until set. (Auto-DPI D3.)
    pub(crate) dpi_scale: f32,
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

/// The focused orrery's render snapshot: the pane rect the element sits at and the
/// focus card. The `.gnode` DOM now lives in a retained host-owned pool under the
/// orrery element, reconciled directly against the shell DOM each frame. (Phase 2.)
#[derive(PartialEq)]
pub(crate) struct OrreryRender {
    /// The orrery pane rect `[x0, y0, x1, y1]` (left, top, right, bottom) in viewport px.
    pub(crate) rect: [f32; 4],
    /// The focused node's content card (snapshot or unvisited placeholder), placed
    /// *after* the gnodes in document order so it paints over them — the spatial
    /// map's nodes sit under the focused content, and the chrome's overlays still paint
    /// over the card (shell order: orrery before chrome). `None` when no node is focused
    /// or the focused node is itself an open tile. (Layering fix — card over nodes.)
    pub(crate) focus_card: Option<FocusCard>,
}

/// The focused node's content card in the shell document: a positioned element placed
/// after the gnodes so document order paints it over them. A `Snapshot` carries a PNG
/// data-URI `<img>` of the page's top peek (built host-side, cached per member while its
/// current URL still matches); an `Unvisited` is a pure-DOM dashed placeholder.
/// (Layering fix — card over nodes.)
#[derive(PartialEq)]
pub(crate) struct FocusCard {
    /// The card rect `[x0, y0, x1, y1]` local to the orrery element origin.
    pub(crate) rect: [f32; 4],
    pub(crate) kind: FocusCardKind,
}

/// One cached preview image for a focused-node snapshot card: the member's current URL plus the
/// chrome `<img>` data URI rendered for it. The cache is window-local; the node thumbnail in the
/// graph is the durable substrate.
pub(crate) struct SnapshotDataUri {
    pub(crate) url: String,
    pub(crate) data_uri: String,
}

#[derive(PartialEq)]
pub(crate) enum FocusCardKind {
    /// A visited node's "last visit" preview: the page's top peek rendered to a PNG
    /// data-URI image, placed as a chrome DOM `<img>` after the gnodes (so it paints
    /// over them and under the overlays — like the favicons already do). An
    /// external-texture cannot serve here: textures composite in the content layer below
    /// the chrome, and the transparent hole does not erase the opaque gnodes behind it.
    /// Only built once the host's per-member cache has an image for the node's current URL,
    /// so there is no empty placeholder while it builds. (Layering fix; no placeholder flash.)
    Snapshot { data_uri: String },
    /// A never-visited node: a static dashed "double-click to load" placeholder.
    Unvisited,
    /// The per-object action card, summoned in place of the preview by a context action: an
    /// ordered list of setting widgets for the selected object's type. Each widget's controls
    /// queue an `object_card_keys` activation the host drains. (Object card — P1.)
    ObjectCard { widgets: Vec<CardWidget> },
    /// The connections swatch: a multi-node selection's nodes plus their inter-edges, laid out as
    /// DOM in the card (`swatch::connections_swatch_view`). Summoned in place of the single-node
    /// preview when `selected_members().len() > 1`. (Swatch primitive — P2, scope=Selection.)
    Connections {
        spec: crate::swatch::ConnectionsSpec,
    },
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
    /// The gloss outline lens's view state (rows + metrics), folded into the shell
    /// document like the roster — the first DOM gloss section. (gloss-outline plan P1.)
    pub(crate) gloss_outline: GlossOutlineState,
    /// The outline's window rect `[x0,y0,x1,y1]`, `Some` while the gloss pane is open;
    /// the shell view positions the outline subtree there (the gloss pane's middle
    /// third — [`gloss::gloss_sections`]). `None` keeps it out of the document.
    pub(crate) gloss_outline_rect: Option<[f32; 4]>,
    /// The gloss recent-visited lens's view state, folded into the shell document like
    /// the outline. (Scene-to-DOM migration P1.)
    pub(crate) gloss_recent: GlossRecentState,
    /// The recent section's window rect, `Some` while the gloss pane is open; positions
    /// the recent subtree at the gloss pane's bottom third
    /// ([`gloss::gloss_sections`]).
    pub(crate) gloss_recent_rect: Option<[f32; 4]>,
    /// The gloss minimap's view state (DOM node squares; the edges/rings backdrop
    /// rides an embedded `<external-texture>` inside this same subtree, not a
    /// separate host composite). (Scene-to-DOM migration P2.)
    pub(crate) gloss_minimap: GlossMinimapState,
    /// The minimap's window rect, `Some` while the gloss pane is open; positions the
    /// minimap subtree at the gloss pane's top third ([`gloss::gloss_sections`]).
    pub(crate) gloss_minimap_rect: Option<[f32; 4]>,
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
    pub(crate) object_card_keys: Vec<String>,
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
const PANE_WRAPPER_CLASS: [&str; 5] = [
    "apparatus-pane",
    "steward-pane",
    "inspector-pane",
    "trail-pane",
    "alembic-pane",
];

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

mod gnode_pool;
mod state_impl;
mod views;

pub(crate) use gnode_pool::*;
pub(crate) use views::*;
