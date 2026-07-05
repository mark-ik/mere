/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! A self-contained, view-driven pane: a [`ServalAppRunner`] over its own DOM, a
//! cached cascade+layout [`PaneSession`], and the resolved stylesheet. This is the
//! pelt `pelt-desktop::chrome::Chrome` bundle, generalized over the pane's state /
//! view / logic so every list-shaped pane (roster, apparatus, the utility panes)
//! shares the render + hit-test + dispatch mechanics. Each pane's own module
//! supplies the `State`, the `view` function, the click intents, and any
//! scroll / a11y-bounds specifics on top of this.
//!
//! It replaces the old per-frame `build_*_dom` rebuild + the hand-maintained
//! `*_rects` hit-test caches: rows/buttons carry `on_click` handlers, so input
//! dispatches through the DOM and there is no rect cache to drift.
//! (Window composition P2 companion — list-pane view-ification.)

use std::cell::RefCell;
use std::rc::Rc;

use netrender::Scene;
use serval_layout::{FragmentPlane, ScrollOffsets};
use serval_scripted_dom::{NodeId, ScriptedDom};
use xilem_serval::{PointerClick, ServalAppRunner, ServalCtx, ServalElement, View};

use crate::pane_session::PaneSession;

/// The runner + cached layout + sheet bundle. `State` is the pane's view state,
/// `Logic` its `view` function, `V` the erased view it returns (a
/// `Box<dyn AnyView<State, (), ServalCtx, ServalElement>>` in practice).
pub struct ViewPane<State, Logic, V>
where
    State: 'static,
    Logic: FnMut(&State) -> V,
    V: View<State, (), ServalCtx, Element = ServalElement>,
{
    runner: ServalAppRunner<State, Logic, V>,
    session: Option<PaneSession>,
    sheets: Vec<String>,
}

impl<State, Logic, V> ViewPane<State, Logic, V>
where
    State: 'static,
    Logic: FnMut(&State) -> V,
    V: View<State, (), ServalCtx, Element = ServalElement>,
{
    /// Build the pane over a fresh DOM, driven by `logic` from `state`.
    pub fn new(logic: Logic, state: State) -> Self {
        let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
        Self {
            runner: ServalAppRunner::new(dom, logic, state),
            session: None,
            sheets: Vec::new(),
        }
    }

    /// Replace the resolved stylesheet (themed each frame by the host).
    pub fn set_sheets(&mut self, sheets: Vec<String>) {
        self.sheets = sheets;
    }

    /// Mutate the view state; the runner re-renders + diffs the change into the DOM.
    pub fn update(&mut self, f: impl FnOnce(&mut State)) {
        self.runner.update(f);
    }

    /// Render the pane to a scene at `w`×`h` with the given scroll offsets, reusing
    /// the cached layout (rebuilt only on a structural / resize / theme change).
    pub fn frame(&mut self, w: u32, h: u32, scroll: &ScrollOffsets<NodeId>) -> Scene {
        let sheet: Vec<&str> = self.sheets.iter().map(String::as_str).collect();
        let dom = self.runner.dom();
        // List-pane sheets are single-mode (built from the ACTIVE mode's
        // tokens, no baked scheme pair yet), so they always evaluate at the
        // base scheme; a mode flip changes their sheet strings and re-themes
        // via the rebuild path. Pair-baking these sheets so they ride the
        // cheap flip too is the theme-modes follow-up.
        PaneSession::scene(&mut self.session, &dom, &sheet, false, w, h, None, scroll)
    }

    /// Hit-test pane-local `(x, y)` against the cached layout, or `None` if the pane
    /// hasn't laid out yet / nothing covers the point.
    pub fn hit_test(&self, x: f32, y: f32, scroll: &ScrollOffsets<NodeId>) -> Option<NodeId> {
        let session = self.session.as_ref()?;
        let dom = self.runner.dom();
        let dom = dom.borrow();
        session.hit_test(&dom, x, y, scroll)
    }

    /// Dispatch a click that hit `node`, firing its `on_click` (which the pane's
    /// handlers use to queue intents); the host then drains them off `state`.
    pub fn dispatch_click(&mut self, node: NodeId, event: PointerClick) {
        self.runner.dispatch_click(node, event);
    }

    /// The cached layout's fragment plane — for reading element bounds (e.g. row
    /// bounds for the a11y projection). `None` before the first frame.
    pub fn fragments(&self) -> Option<&FragmentPlane<NodeId>> {
        self.session.as_ref().map(|s| s.fragments())
    }

    /// The scrollable extent `(mx, my)` of container `node` from the engine's `scroll_extent`
    /// — for the test panes' overflow assertions. `None` before the first frame.
    #[cfg(test)]
    pub fn scroll_extent(&self, node: NodeId) -> Option<(f32, f32)> {
        let session = self.session.as_ref()?;
        let dom = self.runner.dom();
        let dom = dom.borrow();
        Some(session.scroll_extent(&dom, node))
    }

    /// The pane DOM handle (for bounds reads that need element attributes).
    pub fn dom(&self) -> Rc<RefCell<ScriptedDom>> {
        self.runner.dom()
    }
}
