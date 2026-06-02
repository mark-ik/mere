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
    el, lens, on_click, text_field_typed, AnyView, PointerClick, ServalAppRunner, ServalCtx,
    ServalElement, TextField, TextInput,
};

/// Meerkat's chrome app state.
///
/// Holds the reused, host-neutral [`ToolbarState`] view-model plus the live
/// omnibar editor. meerkat renders these as DOM and owns the mutations — the
/// host-widget half of the M4 contract. Later slices fold in the omnibar session,
/// command palette, and frame model (all already-built `chrome` types).
pub struct Chrome {
    /// The toolbar's session state — location bar, load status, nav-capability
    /// flags. Reused verbatim from `chrome::toolbar`; the canonical sink the
    /// omnibar submits into.
    pub toolbar: ToolbarState,
    /// The omnibar's live editing buffer (caret / selection / IME), edited by the
    /// `text_field`. `xilem_serval` text editing rides a `TextInput`, while the
    /// reused `ToolbarState.editable.location` is a `String`; the host syncs the
    /// buffer into the session state on submit (Enter), keeping the domain
    /// unchanged.
    pub omnibar: TextInput,
}

impl Chrome {
    /// A chrome state seeded with `initial_location` in both the reused toolbar
    /// session state and the live omnibar buffer.
    pub fn new(initial_location: impl Into<String>) -> Self {
        let location = initial_location.into();
        Self {
            toolbar: ToolbarState::with_initial_location(location.clone()),
            omnibar: TextInput::new(location),
        }
    }
}

/// The erased view type meerkat's logic produces, so the toolbar's concrete
/// `El<…>` tuple need not be spelled (it grows as the chrome does).
pub type ChromeView = Box<dyn AnyView<Chrome, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: chrome state → chrome view tree.
pub type ChromeLogic = fn(&Chrome) -> ChromeView;

/// Navigate back. Stub for now (real history wiring lands with the session
/// runtime); it resets the omnibar to a visible placeholder so the click
/// round-trip is observable on screen.
fn go_back(c: &mut Chrome, _: PointerClick) {
    c.omnibar = TextInput::new("(back)");
}

/// Navigate forward. Stub, mirroring [`go_back`].
fn go_forward(c: &mut Chrome, _: PointerClick) {
    c.omnibar = TextInput::new("(forward)");
}

/// The toolbar chrome as serval DOM: back / forward buttons and an **editable**
/// omnibar — a reused `xilem_serval` [`text_field`](xilem_serval::text_field)
/// over [`Chrome::omnibar`], composed via [`lens`] exactly like pelt-live's
/// field. The host paints its caret and syncs it into the reused `ToolbarState`
/// on submit.
///
/// The chrome-as-DOM seam — meerkat is the next host widget over the graphshell
/// chrome domain, after the egui and iced toolbars.
pub fn chrome_view(_c: &Chrome) -> ChromeView {
    let back = on_click(
        el::<_, Chrome, ()>("button", "back"),
        go_back as fn(&mut Chrome, PointerClick),
    );
    let forward = on_click(
        el::<_, Chrome, ()>("button", "forward"),
        go_forward as fn(&mut Chrome, PointerClick),
    );
    // The omnibar text_field, lensed onto `Chrome::omnibar`. `text_field_typed`
    // names its concrete view so the `lens` projection can be a `fn` pointer
    // (no captured borrow), as in pelt-live.
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_omnibar: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.omnibar;
    let omnibar = lens(make, to_omnibar);
    Box::new(el::<_, Chrome, ()>("div", (back, forward, omnibar)).attr("class", "toolbar"))
}

/// Sync the live omnibar buffer into the reused `ToolbarState` on submit: copy
/// the edited text into `editable.location` and raise the one-shot
/// `location_submitted` signal (the session-runtime consumer clears it after
/// dispatching the navigation). The host calls this on Enter in the focused
/// omnibar; the domain stays `String`-based and unchanged.
pub fn submit_omnibar(c: &mut Chrome) {
    c.toolbar.editable.location = c.omnibar.text().to_string();
    c.toolbar.editable.location_submitted = true;
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

    /// A back-button click routes through the runner and resets the omnibar —
    /// proving the host-owns-mutations half of the contract on the live editor.
    #[test]
    fn back_click_resets_omnibar() {
        let mut runner = runner("mere://welcome");
        let root = runner.root();
        let back = {
            let dom = runner.dom();
            let dom = dom.borrow();
            first_tag(&dom, root, "button").expect("a back button")
        };
        runner.dispatch_click(back, PointerClick::at((0.0, 0.0)));
        assert_eq!(runner.state().omnibar.text(), "(back)");
    }

    /// Submitting the omnibar syncs the live buffer into the reused
    /// `ToolbarState` — the editor edits a `TextInput`, the session state stays
    /// the domain's `String` (location + the one-shot submit signal).
    #[test]
    fn submit_syncs_omnibar_into_toolbar_state() {
        let mut runner = runner("mere://welcome");
        runner.update(|c| {
            c.omnibar = TextInput::new("https://example.test");
            submit_omnibar(c);
        });
        assert_eq!(runner.state().toolbar.editable.location, "https://example.test");
        assert!(runner.state().toolbar.editable.location_submitted);
    }

    /// The painted omnibar caret tracks byte offsets correctly in the live
    /// `<input>` (built via ScriptedDom, not html5ever — which voids `<input>`):
    /// byte 0 sits at the text start, x increases monotonically per char, and a
    /// caret moved mid-text (the `before` + empty preedit span + `after` split)
    /// lands at the *same* x as the continuous reference. Guards the caret
    /// byte→x mapping that a reported "off-by-one" turned out not to violate (the
    /// apparent offset is the caret-at-advance position, e.g. a period's narrow
    /// ink under a full-width advance).
    #[test]
    fn omnibar_caret_tracks_bytes() {
        use pelt_live::caret_screen_rect;
        use xilem_serval::{Key, KeyEvent, NamedKey};
        const TEST_SHEET: &[&str] =
            &["div, button, input { display: block; } input { font-size: 22px; }"];
        let input_of = |runner: &ServalAppRunner<Chrome, ChromeLogic, ChromeView>| {
            let root = runner.root();
            let d = runner.dom();
            let d = d.borrow();
            first_tag(&d, root, "input").expect("input")
        };
        let caret_x = |runner: &ServalAppRunner<Chrome, ChromeLogic, ChromeView>,
                       node: NodeId,
                       byte: usize| {
            caret_screen_rect(&runner.dom().borrow(), TEST_SHEET, 1024, 600, node, byte)
                .map(|r| r.0)
                .expect("caret rect")
        };

        let mut runner = runner("");
        let input = input_of(&runner);
        runner.dispatch_click(input, PointerClick::at((0.0, 0.0)));
        for ch in "abXcd".chars() {
            runner.dispatch_key(KeyEvent::new(Key::Character(ch.to_string())));
        }
        let input = input_of(&runner);

        // End structure (before = full "abXcd"): byte 0 at the start, monotonic.
        let xs: Vec<f32> = (0..=5).map(|b| caret_x(&runner, input, b)).collect();
        assert!(xs[0].abs() < 0.1, "caret at byte 0 is the text start, got {}", xs[0]);
        for w in xs.windows(2) {
            assert!(w[1] > w[0], "caret x advances per char: {:?}", xs);
        }
        let reference_byte_2 = xs[2];

        // Split structure: move the caret mid-text (left 3 → byte 2), so the field
        // renders before="ab" + empty span + after="Xcd". The painted caret must
        // match the continuous reference — the split does not shift it.
        for _ in 0..3 {
            runner.dispatch_key(KeyEvent::new(Key::Named(NamedKey::ArrowLeft)));
        }
        let byte = runner.state().omnibar.caret_byte_in_render();
        assert_eq!(byte, 2, "three lefts from end of \"abXcd\" lands the caret at byte 2");
        let split_x = caret_x(&runner, input, byte);
        assert!(
            (split_x - reference_byte_2).abs() < 0.1,
            "split-structure caret x ({split_x}) matches the continuous reference ({reference_byte_2})"
        );
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
