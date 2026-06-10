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

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use forme::GraphMemberId;
use frame::{FrameLayout, PaneId, SessionId, SplitAxis, SplitChoice};
use winit::window::CursorIcon;

use super::{CachedTile, ContentPane, ResizeDrag};

/// State owned by a single window's view. Methods on `App` reach it through
/// `self.view`; when the window registry lands (MW2) the render / input paths
/// take `&mut WindowView` for the target window explicitly.
#[derive(Default)]
pub(crate) struct WindowView {
    /// Each switcher row's on-screen rect this frame: a click switches to it.
    pub(crate) session_row_rects: Vec<(SessionId, [f32; 4])>,
    /// Each switcher row's close (×) hit rect this frame: a click trashes it.
    pub(crate) session_close_rects: Vec<(SessionId, [f32; 4])>,
    /// The "+" new-graph tile rect this frame, if the switcher is shown.
    pub(crate) session_add_rect: Option<[f32; 4]>,
    /// Each roster row's on-screen rect this frame (node id): a press focuses it.
    pub(crate) roster_row_rects: Vec<(GraphMemberId, [f32; 4])>,
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
