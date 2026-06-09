/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use super::*;
use super::command::Command;
use layout_dom_api::LayoutDom;
use serval_scripted_dom::{NodeId, ScriptedDom};
use xilem_serval::{Key, KeyEvent, NamedKey, PointerClick, ServalAppRunner, TextInput};

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
    assert_eq!(count_tag(&dom, root, "button"), 3, "back + forward + workbench buttons");
    assert_eq!(count_tag(&dom, root, "input"), 1, "the omnibar input");
    // chrome container + toolbar row + sync chip + (empty, closed) suggestions.
    assert_eq!(count_tag(&dom, root, "div"), 4, "chrome + toolbar + sync-chip + suggestions");
}

/// A back-button click records a one-shot history step for the host to apply to
/// the **focused node's own** history (per-node navigation, the node-lineage
/// model). The chrome no longer owns a linear history and does not self-navigate;
/// `content_location` is unchanged until the host drains the step via the orrery.
#[test]
fn back_click_records_a_history_step() {
    let mut runner = runner("mere://welcome");
    // Navigate to a typed bare host (normalized to https://).
    runner.update(|c| {
        c.omnibar = TextInput::new("example.com");
        submit_omnibar(c);
    });
    assert_eq!(runner.state().content_location(), "https://example.com");
    assert!(runner.state().history_step.is_none());

    let root = runner.root();
    let back = {
        let dom = runner.dom();
        let dom = dom.borrow();
        first_tag(&dom, root, "button").expect("a back button")
    };
    runner.dispatch_click(back, PointerClick::at((0.0, 0.0)));

    // The chrome recorded the intent; it did not self-navigate.
    assert_eq!(runner.state().history_step, Some(HistoryStep::Back));
    assert_eq!(runner.state().content_location(), "https://example.com");
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

/// Plain omnibar submit does not raise the open-as-new-node intent — only
/// Ctrl/Cmd-Enter does (the host's key handler sets it). Guards that in-place
/// navigation stays the default path for a bare Enter.
#[test]
fn plain_submit_does_not_open_a_new_node() {
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.omnibar = TextInput::new("example.com");
        submit_omnibar(c);
    });
    assert_eq!(runner.state().content_location(), "https://example.com");
    assert!(!runner.state().open_as_new_node, "plain Enter navigates in place, not a new node");
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

/// Toggling opens then closes the palette (the Ctrl+K path).
#[test]
fn palette_toggles_open_and_closed() {
    let mut runner = runner("mere://welcome");
    runner.update(Chrome::toggle_palette);
    assert!(runner.state().palette_open);
    runner.update(Chrome::toggle_palette);
    assert!(!runner.state().palette_open);
}

/// An open palette renders its panel and one row per filtered command
/// (all three at an empty query) — the reused session driving meerkat's
/// command set into the DOM.
#[test]
fn palette_open_renders_rows() {
    let mut runner = runner("mere://welcome");
    runner.update(Chrome::open_palette);
    let dom = runner.dom();
    let dom = dom.borrow();
    let root = runner.root();
    assert_eq!(count_class(&dom, root, "palette"), 1, "the panel");
    assert_eq!(
        count_class(&dom, root, "cmd-row"),
        13,
        "chrome verbs + Tile / Delete / Background / Hide edge / Show edges / Settings / Comms / Inspector / Steward",
    );
}

/// The palette filters by query and runs the match: after navigating away,
/// filtering to "back" and submitting steps history back and closes.
#[test]
fn palette_filters_and_runs_command() {
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.omnibar = TextInput::new("example.com");
        submit_omnibar(c);
    });
    assert_eq!(runner.state().content_location(), "https://example.com");

    runner.update(|c| {
        c.open_palette();
        c.palette_input = TextInput::new("back");
        c.sync_palette_query();
    });
    // "back" matches Back and "...active in background" (substring).
    assert_eq!(
        runner.state().palette_commands(),
        vec![Command::Back, Command::BackgroundNode],
    );

    // No explicit highlight → run_palette_selection runs the first match (Back).
    runner.update(Chrome::run_palette_selection);
    // Command::Back records a per-node history step for the host (the chrome does
    // not self-navigate), and running closes the palette.
    assert_eq!(runner.state().history_step, Some(HistoryStep::Back));
    assert!(!runner.state().palette_open, "running closes the palette");
}

/// Palette selection stepping wraps via the reused `step_selection`.
#[test]
fn palette_step_wraps() {
    let mut runner = runner("mere://welcome");
    runner.update(Chrome::open_palette);
    runner.update(|c| c.step_palette(-1));
    assert_eq!(runner.state().palette.selected_index, Some(Command::ALL.len() - 1), "up from none → last");
    runner.update(|c| c.step_palette(1));
    assert_eq!(runner.state().palette.selected_index, Some(0), "wrap to first");
}

/// The settings overlay opens, renders its panel + the cap controls, and the
/// − / + buttons edit the tab cap (the host applies + persists it).
#[test]
fn settings_overlay_opens_and_edits_the_tab_cap() {
    let mut runner = runner("mere://welcome");
    runner.update(Chrome::open_settings);
    assert!(runner.state().settings_open, "the overlay opens");
    {
        let dom = runner.dom();
        let dom = dom.borrow();
        assert_eq!(count_class(&dom, runner.root(), "settings"), 1, "the panel renders");
        assert_eq!(count_class(&dom, runner.root(), "set-btn"), 3, "− / + cap buttons + close ×");
    }
    let before = runner.state().settings.tab_cap;
    runner.update(Chrome::inc_tab_cap);
    assert_eq!(runner.state().settings.tab_cap, before + 1, "+ raises the cap");
    runner.update(Chrome::dec_tab_cap);
    runner.update(Chrome::dec_tab_cap);
    assert_eq!(runner.state().settings.tab_cap, before - 1, "- lowers it");
    runner.update(Chrome::close_settings);
    assert!(!runner.state().settings_open, "and it closes");
}

/// The context menu renders a row per item, and a row click captures its action
/// (closing the menu) for the host to drain.
#[test]
fn context_menu_renders_and_captures_an_action() {
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.open_context_menu(
            120.0,
            240.0,
            vec![
                ContextItem::new("Open in splits", ContextAction::OpenSplits),
                ContextItem::new("Open in a stack", ContextAction::Stack),
            ],
        );
    });
    assert!(runner.state().context_menu.is_some(), "the menu opens");
    {
        let dom = runner.dom();
        let dom = dom.borrow();
        assert_eq!(count_class(&dom, runner.root(), "context-menu"), 1, "the panel renders");
        assert_eq!(count_class(&dom, runner.root(), "context-item"), 2, "a row per item");
    }
    // Picking a row captures its action and closes the menu.
    runner.update(|c| c.pick_context(ContextAction::Stack));
    assert_eq!(runner.state().pending_context, Some(ContextAction::Stack));
    assert!(runner.state().context_menu.is_none(), "the menu closes on pick");
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
