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
//! The toolbar renders from a reused [`chrome::toolbar::ToolbarState`] into a
//! serval `ScriptedDom` via [`ServalAppRunner`], on screen, with an editable
//! omnibar ([`TextInput`] lensed into the view). Submitting (Enter) and the
//! back / forward buttons drive a real linear [`History`](nav::History): the
//! omnibar text is classified and resolved to a URL, pushed onto the stack, and
//! mirrored back into the toolbar (location text, `can_go_*` flags). The current
//! history entry — [`Chrome::content_location`] — is what the **content-root**
//! slice (next) will load into a separate document authority.

use std::cell::RefCell;
use std::rc::Rc;

use chrome::omnibar::OmnibarMatch;
use chrome::toolbar::ToolbarState;
use serval_scripted_dom::ScriptedDom;
use xilem_serval::{
    el, lens, on_click, text_field_typed, AnyView, PointerClick, ServalAppRunner, ServalCtx,
    ServalElement, TextField, TextInput,
};

pub mod content;
pub mod nav;
pub mod suggest;

use nav::History;

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
    /// meerkat's own linear back/forward history. graphshell projects the
    /// toolbar's `can_go_*` flags from a servo viewer's history; meerkat has no
    /// such viewer, so it owns the stack and mirrors the flags into
    /// [`ToolbarState`]. Its current entry is the URL the content root shows.
    pub history: History,
    /// Live omnibar suggestions (history + search), in the reused
    /// [`OmnibarMatch`] vocabulary. Empty ⇒ the dropdown is closed. Regenerated
    /// from the omnibar text on each edit via [`Chrome::refresh_suggestions`].
    pub suggest: Vec<OmnibarMatch>,
    /// Highlighted suggestion row, if any. `None` ⇒ Enter navigates the typed
    /// text; `Some(i)` ⇒ Enter (or a click) navigates `suggest[i]`.
    pub suggest_active: Option<usize>,
}

impl Chrome {
    /// A chrome state seeded with `initial_location` (classified the same way a
    /// submission is, so a bare host normalizes) across the history, the reused
    /// toolbar session state, and the live omnibar buffer.
    pub fn new(initial_location: impl Into<String>) -> Self {
        let raw = initial_location.into();
        // A blank cold-start stays blank (not a search for the empty string).
        let location = if raw.trim().is_empty() {
            String::new()
        } else {
            nav::classify(&raw).resolve()
        };
        Self {
            toolbar: ToolbarState::with_initial_location(location.clone()),
            omnibar: TextInput::new(location.clone()),
            history: History::new(location),
            suggest: Vec::new(),
            suggest_active: None,
        }
    }

    /// The URL the content root should display — the current history entry. The
    /// content-root slice loads this; the chrome shell never renders it itself.
    pub fn content_location(&self) -> &str {
        self.history.current()
    }

    /// Regenerate the omnibar suggestions from the current omnibar text and the
    /// history, resetting the highlight. The host calls this after each omnibar
    /// edit (keystroke / caret move).
    pub fn refresh_suggestions(&mut self) {
        self.suggest = suggest::suggestions(self.omnibar.text(), &self.history);
        self.suggest_active = None;
    }

    /// Move the suggestion highlight by `delta` (wrapping), opening the
    /// highlight at the first/last row when nothing is highlighted yet. A no-op
    /// when there are no suggestions.
    pub fn step_suggestion(&mut self, delta: isize) {
        let count = self.suggest.len();
        if count == 0 {
            self.suggest_active = None;
            return;
        }
        let n = count as isize;
        self.suggest_active = Some(match self.suggest_active {
            Some(cur) => ((cur as isize + delta).rem_euclid(n)) as usize,
            None if delta >= 0 => 0,
            None => (n - 1) as usize,
        });
    }

    /// Close the suggestion dropdown without navigating.
    pub fn close_suggestions(&mut self) {
        self.suggest.clear();
        self.suggest_active = None;
    }

    /// Navigate to suggestion row `i` (clicked or Enter-on-highlight): resolve
    /// its URL, push history, and mirror into the toolbar + omnibar.
    pub fn navigate_suggestion(&mut self, i: usize) {
        if let Some(url) = self.suggest.get(i).and_then(suggest::resolve_match) {
            self.history.visit(url);
            sync_chrome_from_history(self, true);
        }
    }
}

/// The erased view type meerkat's logic produces, so the toolbar's concrete
/// `El<…>` tuple need not be spelled (it grows as the chrome does).
pub type ChromeView = Box<dyn AnyView<Chrome, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: chrome state → chrome view tree.
pub type ChromeLogic = fn(&Chrome) -> ChromeView;

/// Navigate back one history entry, mirroring the new current location into the
/// toolbar and omnibar. A no-op (no chrome change) when already at the oldest
/// entry, so a disabled Back button click does nothing.
fn go_back(c: &mut Chrome, _: PointerClick) {
    if c.history.back().is_some() {
        sync_chrome_from_history(c, false);
    }
}

/// Navigate forward one history entry. Mirror of [`go_back`].
fn go_forward(c: &mut Chrome, _: PointerClick) {
    if c.history.forward().is_some() {
        sync_chrome_from_history(c, false);
    }
}

/// Mirror the current history entry into the reused chrome state: the toolbar
/// location text and `can_go_*` flags, and the live omnibar buffer (so the bar
/// shows the resolved URL after a navigation). `submitted` raises the one-shot
/// `location_submitted` chrome signal — set for an omnibar Enter, cleared for a
/// back/forward step (which is not a fresh user submission).
///
/// `load_status` is intentionally left untouched here: until the content-root
/// engine reports real progress, forcing `Started` would strand the toolbar in
/// a permanent false "loading" state.
fn sync_chrome_from_history(c: &mut Chrome, submitted: bool) {
    let url = c.history.current().to_string();
    c.toolbar.editable.location = url.clone();
    c.toolbar.editable.location_dirty = false;
    c.toolbar.editable.location_submitted = submitted;
    c.toolbar.can_go_back = c.history.can_back();
    c.toolbar.can_go_forward = c.history.can_forward();
    c.omnibar = TextInput::new(url);
    c.close_suggestions();
}

/// The toolbar chrome as serval DOM: back / forward buttons and an **editable**
/// omnibar — a reused `xilem_serval` [`text_field`](xilem_serval::text_field)
/// over [`Chrome::omnibar`], composed via [`lens`] exactly like pelt-live's
/// field. The host paints its caret and syncs it into the reused `ToolbarState`
/// on submit.
///
/// The chrome-as-DOM seam — meerkat is the next host widget over the graphshell
/// chrome domain, after the egui and iced toolbars.
pub fn chrome_view(c: &Chrome) -> ChromeView {
    // Reflect the reused nav-capability flags onto the buttons: a spent
    // direction carries a `disabled` class (the host sheet greys it; the
    // handler is already a no-op at the history's edge).
    let back_class = if c.toolbar.can_go_back { "nav" } else { "nav disabled" };
    let forward_class = if c.toolbar.can_go_forward {
        "nav"
    } else {
        "nav disabled"
    };
    let back = on_click(
        el::<_, Chrome, ()>("button", "back").attr("class", back_class),
        go_back as fn(&mut Chrome, PointerClick),
    );
    let forward = on_click(
        el::<_, Chrome, ()>("button", "forward").attr("class", forward_class),
        go_forward as fn(&mut Chrome, PointerClick),
    );
    // The omnibar text_field, lensed onto `Chrome::omnibar`. `text_field_typed`
    // names its concrete view so the `lens` projection can be a `fn` pointer
    // (no captured borrow), as in pelt-live.
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_omnibar: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.omnibar;
    let omnibar = lens(make, to_omnibar);
    let toolbar =
        el::<_, Chrome, ()>("div", (back, forward, omnibar)).attr("class", "toolbar");

    // The suggestions dropdown: one row per reused `OmnibarMatch`, the highlight
    // carrying a distinct class. Empty ⇒ a zero-height `div` (closed). The outer
    // `.chrome` container has no background; the host composites it over the
    // content root, so the toolbar and this dropdown float above the page.
    let rows: Vec<ChromeView> = c
        .suggest
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let class = if c.suggest_active == Some(i) {
                "suggestion-active"
            } else {
                "suggestion"
            };
            let row = on_click(
                el::<_, Chrome, ()>("div", suggest::match_label(m)).attr("class", class),
                move |c: &mut Chrome, _: PointerClick| c.navigate_suggestion(i),
            );
            Box::new(row) as ChromeView
        })
        .collect();
    let suggestions = el::<_, Chrome, ()>("div", rows).attr("class", "suggestions");

    Box::new(el::<_, Chrome, ()>("div", (toolbar, suggestions)).attr("class", "chrome"))
}

/// Navigate on submit (Enter). If a suggestion row is highlighted, navigate it;
/// otherwise classify the typed text into a [`NavTarget`](nav::NavTarget),
/// resolve it to a URL, push history, and mirror into the reused `ToolbarState`
/// + omnibar. Empty input with no highlight is a no-op. The host calls this on
/// Enter in the focused omnibar; the reused domain stays `String`-based.
pub fn submit_omnibar(c: &mut Chrome) {
    if let Some(i) = c.suggest_active {
        c.navigate_suggestion(i);
        return;
    }
    if c.omnibar.text().trim().is_empty() {
        return;
    }
    let url = nav::classify(c.omnibar.text()).resolve();
    c.history.visit(url);
    sync_chrome_from_history(c, true);
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
    /// two buttons (back / forward) and one omnibar input, inside the chrome /
    /// toolbar / (empty) suggestions div scaffold. The reuse smoke test — the
    /// graphshell chrome domain renders through `xilem_serval`.
    #[test]
    fn toolbar_renders_from_reused_state() {
        let runner = runner("mere://welcome");
        let dom = runner.dom();
        let dom = dom.borrow();
        let root = runner.root();
        assert_eq!(count_tag(&dom, root, "button"), 2, "back + forward buttons");
        assert_eq!(count_tag(&dom, root, "input"), 1, "the omnibar input");
        // chrome container + toolbar row + (empty, closed) suggestions div.
        assert_eq!(count_tag(&dom, root, "div"), 3, "chrome + toolbar + suggestions");
    }

    /// A back-button click steps the real history: after navigating away from
    /// the seed location, Back returns to it, updates the omnibar to the
    /// restored URL, and flips the `can_go_*` flags (forward now available,
    /// back spent). Proves the host-owns-mutations contract on live navigation.
    #[test]
    fn back_click_navigates_history() {
        let mut runner = runner("mere://welcome");
        // Navigate to a typed bare host (normalized to https://).
        runner.update(|c| {
            c.omnibar = TextInput::new("example.com");
            submit_omnibar(c);
        });
        assert_eq!(runner.state().content_location(), "https://example.com");
        assert!(runner.state().toolbar.can_go_back);
        assert!(!runner.state().toolbar.can_go_forward);

        let root = runner.root();
        let back = {
            let dom = runner.dom();
            let dom = dom.borrow();
            first_tag(&dom, root, "button").expect("a back button")
        };
        runner.dispatch_click(back, PointerClick::at((0.0, 0.0)));

        assert_eq!(runner.state().content_location(), "mere://welcome");
        assert_eq!(runner.state().omnibar.text(), "mere://welcome");
        assert!(!runner.state().toolbar.can_go_back);
        assert!(runner.state().toolbar.can_go_forward);
    }

    /// Submitting the omnibar syncs the live buffer into the reused
    /// `ToolbarState` — the editor edits a `TextInput`, the session state stays
    /// the domain's `String` (location + the one-shot submit signal) — and the
    /// resolved URL lands as the content location.
    #[test]
    fn submit_syncs_omnibar_into_toolbar_state() {
        let mut runner = runner("mere://welcome");
        runner.update(|c| {
            c.omnibar = TextInput::new("https://example.test");
            submit_omnibar(c);
        });
        assert_eq!(runner.state().toolbar.editable.location, "https://example.test");
        assert!(runner.state().toolbar.editable.location_submitted);
        assert_eq!(runner.state().content_location(), "https://example.test");
    }

    /// An empty submission is a no-op: it neither grows the history nor raises
    /// the submit signal (guards the blank-Enter guard in `submit_omnibar`).
    #[test]
    fn empty_submit_is_a_no_op() {
        let mut runner = runner("mere://welcome");
        runner.update(|c| {
            c.omnibar = TextInput::new("   ");
            submit_omnibar(c);
        });
        assert_eq!(runner.state().content_location(), "mere://welcome");
        assert!(!runner.state().toolbar.can_go_back);
        assert!(!runner.state().toolbar.editable.location_submitted);
    }

    /// Refreshing populates the dropdown from the omnibar text, and the rendered
    /// tree gains one `.suggestion*` row per match — the reused `OmnibarMatch`
    /// types diffing into the DOM (the thread-3 reuse seam).
    #[test]
    fn refresh_renders_suggestion_rows() {
        let mut runner = runner("mere://welcome");
        runner.update(|c| {
            c.omnibar = TextInput::new("example.com");
            c.refresh_suggestions();
        });
        let n = runner.state().suggest.len();
        assert!(n >= 2, "direct-nav + search rows at least, got {n}");
        // Each match becomes a row div (class suggestion / suggestion-active).
        let dom = runner.dom();
        let dom = dom.borrow();
        let rows = count_class(&dom, runner.root(), "suggestion")
            + count_class(&dom, runner.root(), "suggestion-active");
        assert_eq!(rows, n, "one rendered row per suggestion");
    }

    /// Arrow stepping wraps and seeds from either end; refreshing clears the
    /// highlight (reusing the same cursor semantics as the command palette).
    #[test]
    fn step_suggestion_wraps_and_refresh_resets() {
        let mut runner = runner("mere://welcome");
        runner.update(|c| {
            c.omnibar = TextInput::new("example.com");
            c.refresh_suggestions();
        });
        let count = runner.state().suggest.len();
        runner.update(|c| c.step_suggestion(-1));
        assert_eq!(runner.state().suggest_active, Some(count - 1), "up from none → last");
        runner.update(|c| c.step_suggestion(1));
        assert_eq!(runner.state().suggest_active, Some(0), "wrap to first");
        runner.update(Chrome::refresh_suggestions);
        assert_eq!(runner.state().suggest_active, None, "refresh clears the highlight");
    }

    /// Enter on a highlighted suggestion navigates *that* row and closes the
    /// dropdown. Typing `example.com` offers a direct-nav row first; stepping to
    /// it and submitting navigates the resolved URL.
    #[test]
    fn enter_navigates_highlighted_suggestion() {
        let mut runner = runner("mere://welcome");
        runner.update(|c| {
            c.omnibar = TextInput::new("example.com");
            c.refresh_suggestions();
        });
        runner.update(|c| {
            c.step_suggestion(1); // none → first row (the direct-nav URL)
            submit_omnibar(c);
        });
        assert_eq!(runner.state().content_location(), "https://example.com");
        assert!(runner.state().suggest.is_empty(), "navigation closes the dropdown");
    }

    /// Count elements carrying exactly class `class` in the subtree at `id`.
    fn count_class(dom: &ScriptedDom, id: NodeId, class: &str) -> usize {
        let here = usize::from(
            dom.attributes(id)
                .any(|a| a.name.local.as_ref() == "class" && a.value == class),
        );
        here + dom.dom_children(id).map(|c| count_class(dom, c, class)).sum::<usize>()
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
