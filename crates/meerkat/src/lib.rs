/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # meerkat
//!
//! Mere's serval-as-host shell — the chrome (toolbar, omnibar, command palette,
//! frametree) built as [`xilem_serval`] views over the **reused** `graphshell`
//! chrome domain, presented by serval. This is flip Phase 3 (chrome-as-DOM): the
//! eventual replacement for the Xilem + Masonry `mere-app` host.
//!
//! ## Reuse, not rewrite
//!
//! The chrome *model* already exists and is tested: `chrome::toolbar`,
//! `chrome::omnibar`, `chrome::command_palette`, `chrome::frame_model`,
//! `chrome::routing` are host-neutral, WASM-clean view-models built to the
//! contract *"host widgets render from these types through the view-model; the
//! shell owns the mutations."* meerkat is the next such host widget (after the
//! egui and iced ones), so it renders those types as serval DOM and routes
//! mutations back through the runner. Only the *rendering* is new.
//!
//! ## Separate roots
//!
//! From the first commit the **chrome-root** (this view tree, diffed by
//! `xilem_serval` from app state) and each **content-root** (mutated by its
//! engine / script) are distinct document authorities; neither sees the other's
//! tree (flip plan, Phase 3 + Standing constraints).
//!
//! ## Status
//!
//! First slice (headless): the toolbar renders from a reused
//! [`chrome::toolbar::ToolbarState`] into a serval `ScriptedDom` via
//! [`ServalAppRunner`] — proving the cross-repo wiring and the reuse seam. The
//! windowing / present stack (pelt-live-shaped) and the editable omnibar
//! (`TextInput` synced into `editable.location`) land next.

use std::cell::RefCell;
use std::rc::Rc;

use chrome::toolbar::ToolbarState;
use serval_scripted_dom::ScriptedDom;
use xilem_serval::{
    el, on_click, AnyView, PointerClick, ServalAppRunner, ServalCtx, ServalElement,
};

/// Meerkat's chrome app state.
///
/// Holds the reused, host-neutral [`ToolbarState`] view-model. meerkat renders it
/// as DOM and owns the mutations — the host-widget half of the M4 contract.
/// Later slices fold in the omnibar session, command palette, and frame model
/// (all already-built `chrome` types).
pub struct Chrome {
    /// The toolbar's session state — location bar, load status, nav-capability
    /// flags. Reused verbatim from `chrome::toolbar`.
    pub toolbar: ToolbarState,
}

impl Chrome {
    /// A chrome state seeded with `initial_location` in the omnibar (every other
    /// toolbar field at its idle default), via the domain's own constructor.
    pub fn new(initial_location: impl Into<String>) -> Self {
        Self {
            toolbar: ToolbarState::with_initial_location(initial_location),
        }
    }
}

/// The erased view type meerkat's logic produces, so the toolbar's concrete
/// `El<…>` tuple need not be spelled (it grows as the chrome does).
pub type ChromeView = Box<dyn AnyView<Chrome, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: chrome state → chrome view tree.
pub type ChromeLogic = fn(&Chrome) -> ChromeView;

/// Navigate back. Stub for the first slice (real history wiring lands with the
/// session runtime); it records the intent in the location bar so the round-trip
/// state → view → DOM is observable.
fn go_back(c: &mut Chrome, _: PointerClick) {
    c.toolbar.editable.location = "(back)".into();
}

/// Navigate forward. Stub, mirroring [`go_back`].
fn go_forward(c: &mut Chrome, _: PointerClick) {
    c.toolbar.editable.location = "(forward)".into();
}

/// The toolbar chrome as serval DOM, rendered from the reused [`ToolbarState`]:
/// back / forward buttons and the omnibar showing the current location.
///
/// The chrome-as-DOM seam — meerkat is the next host widget over the graphshell
/// chrome domain, after the egui and iced toolbars. For this first slice the
/// omnibar is read-through (it shows `editable.location`); the editable
/// `text_field` (a `TextInput` synced into `editable.location`) lands next.
pub fn chrome_view(c: &Chrome) -> ChromeView {
    let back = on_click(
        el::<_, Chrome, ()>("button", "back"),
        go_back as fn(&mut Chrome, PointerClick),
    );
    let forward = on_click(
        el::<_, Chrome, ()>("button", "forward"),
        go_forward as fn(&mut Chrome, PointerClick),
    );
    let omnibar = el::<_, Chrome, ()>("input", c.toolbar.editable.location.clone())
        .attr("class", "omnibar");
    Box::new(el::<_, Chrome, ()>("div", (back, forward, omnibar)).attr("class", "toolbar"))
}

/// Build the chrome via a [`ServalAppRunner`] over a fresh [`ScriptedDom`] — the
/// same diff path the windowed host will drive, minus layout / paint. Returns the
/// runner so callers (and tests) can inspect the DOM, dispatch input, and rebuild.
pub fn runner(initial_location: &str) -> ServalAppRunner<Chrome, ChromeLogic, ChromeView> {
    let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
    ServalAppRunner::new(dom, chrome_view as ChromeLogic, Chrome::new(initial_location))
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_dom_api::LayoutDom;
    use serval_scripted_dom::NodeId;

    /// Count elements with local tag `tag` in the subtree rooted at `id`.
    fn count_tag(dom: &ScriptedDom, id: NodeId, tag: &str) -> usize {
        let here = usize::from(dom.element_name(id).is_some_and(|q| q.local.as_ref() == tag));
        here + dom.dom_children(id).map(|c| count_tag(dom, c, tag)).sum::<usize>()
    }

    /// The toolbar view diffs into the ScriptedDom from a reused `ToolbarState`:
    /// two buttons (back / forward) and one omnibar input. The reuse smoke test —
    /// the graphshell chrome domain renders through `xilem_serval`.
    #[test]
    fn toolbar_renders_from_reused_state() {
        let runner = runner("mere://welcome");
        let dom = runner.dom();
        let dom = dom.borrow();
        let root = runner.root();
        assert_eq!(count_tag(&dom, root, "button"), 2, "back + forward buttons");
        assert_eq!(count_tag(&dom, root, "input"), 1, "the omnibar input");
        assert_eq!(count_tag(&dom, root, "div"), 1, "the toolbar container");
    }

    /// A back-button click routes through the runner and mutates the reused
    /// `ToolbarState` — proving the host-owns-mutations half of the contract.
    #[test]
    fn back_click_mutates_toolbar_state() {
        let mut runner = runner("mere://welcome");
        // Find the first button (back) and click it.
        let root = runner.root();
        let back = {
            let dom = runner.dom();
            let dom = dom.borrow();
            first_tag(&dom, root, "button").expect("a back button")
        };
        runner.dispatch_click(back, PointerClick::at((0.0, 0.0)));
        assert_eq!(runner.state().toolbar.editable.location, "(back)");
    }

    /// The first element with local tag `tag` in pre-order, if any.
    fn first_tag(dom: &ScriptedDom, id: NodeId, tag: &str) -> Option<NodeId> {
        if dom.element_name(id).is_some_and(|q| q.local.as_ref() == tag) {
            return Some(id);
        }
        for c in dom.dom_children(id) {
            if let Some(found) = first_tag(dom, c, tag) {
                return Some(found);
            }
        }
        None
    }
}
