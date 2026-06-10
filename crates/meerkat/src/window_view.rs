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

use forme::GraphMemberId;
use frame::SessionId;

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
}
