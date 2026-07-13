/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The one-state-N-windows projection types (Slice 3): [`AppState`] (the single state
//! the multi-runner owns) and the runner / view aliases over it.
//!
//! `AppState` holds the window-invariant chrome chips (`shared`) once, plus one
//! [`WindowLocal`] per OS window in `windows`. Each window is a **projection**: the
//! shell view lensed onto `windows[i]`, reading `shared` for the crawl chip. The flip
//! replaced the N per-window `GenetAppRunner`s with this one [`GenetMultiRunner`], so
//! multi-window synced panels stop being a sync feature — there is one state, so there is
//! nothing to synchronize. (One state, N windows — Slice 3.)

use super::*;

/// The single application state the [`ShellMultiRunner`] owns: the shared chrome chips
/// plus every window's local view-state. A window is `windows[projection_id]`; the
/// projection's lens routes into it. Slots are never popped (a closed window tombstones
/// its projection and leaves its `WindowLocal` in place), so a window's index stays equal
/// to its [`ProjectionId`]. (One state, N windows — Slice 3.)
pub(crate) struct AppState {
    /// The window-invariant chrome chips (p2p sync + crawl), owned once. The crawl chip is
    /// read by every window's shell view; sync is read host-side by Steward / Apparatus.
    /// This absorbs Slice 0's `Rc<RefCell<SharedChrome>>` — the multi-runner is the one
    /// owner now, so there is no cell.
    pub(crate) shared: SharedChrome,
    /// One [`WindowLocal`] per OS window, indexed by [`ProjectionId`]. Append-only
    /// (tombstoned on close, never popped), so `windows[pid.0]` is always this projection's
    /// slot.
    pub(crate) windows: Vec<WindowLocal>,
}

/// The erased per-window view, over one window's [`WindowLocal`]. The inner shell builders
/// produce this; [`shell_view`] lenses it up to [`AppState`].
pub(crate) type WindowLocalView = Box<dyn AnyView<WindowLocal, (), GenetCtx, GenetElement>>;

/// The erased shell root view, over the whole [`AppState`] (a per-window lens into
/// `windows[i]`). This is what each projection's logic returns.
pub(crate) type ShellView = Box<dyn AnyView<AppState, (), GenetCtx, GenetElement>>;

/// One projection's view logic: the shell view over [`AppState`], closed over the window's
/// index. Boxed because each window captures a distinct index. (Slice 3.)
pub(crate) type BoxedLogic = Box<dyn FnMut(&AppState) -> ShellView>;

/// The one runner the shell holds: [`AppState`] projected into N windows. (Slice 3.)
pub(crate) type ShellMultiRunner = GenetMultiRunner<AppState, BoxedLogic, ShellView>;

impl WindowLocal {
    /// A window's local view-state at rest, seeded with `chrome`. Everything else starts
    /// empty (no orrery snapshot, closed panes, no drained intents). The primary (boot) and
    /// every spawned window mint one of these before pushing their projection. Replaces the
    /// inline seed the old per-window `shell_runner` built. (Slice 3.)
    pub(crate) fn new(chrome: Chrome) -> Self {
        Self {
            chrome,
            orrery: OrreryRender {
                rect: [0.0; 4],
                focus_card: None,
            },
            roster: RosterState::default(),
            roster_rect: None,
            panes: std::array::from_fn(|_| ListPaneState::default()),
            pane_rects: [None; 5],
            gloss_outline: GlossOutlineState::default(),
            gloss_outline_rect: None,
            gloss_recent: GlossRecentState::default(),
            gloss_recent_rect: None,
            gloss_minimap: GlossMinimapState::default(),
            gloss_minimap_rect: None,
            orrery_wheel: None,
            settings: SettingsPanesState::default(),
            object_card_keys: Vec::new(),
        }
    }
}
