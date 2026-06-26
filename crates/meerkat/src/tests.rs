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
    assert_eq!(count_tag(&dom, root, "button"), 13, "back + forward + pause + add-pill toolbar buttons + 9 shellbar buttons");
    assert_eq!(count_tag(&dom, root, "input"), 1, "the omnibar input");
    // chrome container + toolbar row + branch chip + crawl chip + (empty) suggestions + shellbar.
    // The sync chip moved into the Steward / Apparatus panes (Chrome bar P1).
    assert_eq!(count_tag(&dom, root, "div"), 6, "chrome + toolbar + branch-chip + crawl-chip + suggestions + shellbar");
}

/// Ghost autocomplete in command mode: a partial `>ros` shows the dim `ter`
/// suffix from the shared command vocabulary; accepting completes the buffer to
/// `>roster`; an address (no sigil) shows no ghost. The ghost never enters the
/// committed buffer, so submit evaluates only what was typed.
#[test]
fn omnibar_ghost_completes_command_mode() {
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.omnibar = TextInput::new(">ros");
        c.refresh_suggestions();
    });
    assert_eq!(runner.state().omnibar.ghost(), "ter", "completes >ros -> >roster");
    assert_eq!(runner.state().omnibar.text(), ">ros", "the ghost stays out of the buffer");

    runner.update(|c| {
        c.omnibar.accept_ghost();
        c.refresh_suggestions();
    });
    assert_eq!(runner.state().omnibar.text(), ">roster");
    assert_eq!(runner.state().omnibar.ghost(), "", "a complete verb has no further ghost");

    runner.update(|c| {
        c.omnibar = TextInput::new("example.com");
        c.refresh_suggestions();
    });
    assert_eq!(runner.state().omnibar.ghost(), "", "navigation text is not completed");
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

/// The command palette lists context actions alongside commands (registry P2), and
/// running a context palette item records it in `pending_palette_action` for the host to
/// apply to the current selection (and closes the palette).
#[test]
fn palette_runs_a_context_action_into_the_pending_slot() {
    use crate::command::PaletteItem;
    let mut runner = runner("mere://welcome");
    runner.update(Chrome::open_palette);
    let item = runner
        .state()
        .palette_items()
        .into_iter()
        .find(|i| matches!(i, PaletteItem::Context(ContextAction::ShowAllNodes)))
        .expect("Show all nodes is a palette item");
    runner.update(|c| c.run_palette_item_and_close(item));
    assert_eq!(
        runner.state().pending_palette_action,
        Some(ContextAction::ShowAllNodes),
        "the palette records the context action for the host to apply",
    );
    assert!(!runner.state().palette_open, "running the item closes the palette");
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

/// An open palette renders its panel and one row per filtered palette item — every
/// command plus the palette-exposed context actions (registry P2), at an empty query.
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
        Command::ALL.len() + crate::command::PALETTE_CONTEXT_ACTIONS.len(),
        "one row per palette item (commands + the palette-exposed context actions)",
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
    let count = runner.state().palette_items().len();
    runner.update(|c| c.step_palette(-1));
    assert_eq!(runner.state().palette.selected_index, Some(count - 1), "up from none → last");
    runner.update(|c| c.step_palette(1));
    assert_eq!(runner.state().palette.selected_index, Some(0), "wrap to first");
}

/// Context-menu keyboard nav wraps like the palette, and Enter runs the highlighted row,
/// capturing its action and closing the menu. (Context-menu keyboard nav.)
#[test]
fn context_menu_keyboard_nav_wraps_and_runs() {
    use crate::{ContextAction, ContextItem};
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.open_context_menu(
            10.0,
            10.0,
            vec![
                ContextItem::new("A", ContextAction::AddNode),
                ContextItem::new("B", ContextAction::AddField),
            ],
        )
    });
    // None → last on a step up, then wrap forward to first.
    runner.update(|c| c.step_context_menu(-1));
    assert_eq!(runner.state().context_menu.as_ref().unwrap().selected, Some(1), "up from none → last");
    runner.update(|c| c.step_context_menu(1));
    assert_eq!(runner.state().context_menu.as_ref().unwrap().selected, Some(0), "wrap to first");
    // Enter runs the highlighted row: its action becomes pending and the menu closes.
    runner.update(Chrome::run_context_selection);
    assert!(runner.state().context_menu.is_none(), "running closes the menu");
    assert_eq!(runner.state().pending_context, Some(ContextAction::AddNode));
}

/// The active-tab cap edits within bounds. The overlay that used to host it was retired
/// into the `pelt/appearance` page (Settings lane P2); the cap controls there drain
/// `tiles:cap:up` / `tiles:cap:down` to these same `inc_tab_cap` / `dec_tab_cap` methods.
#[test]
fn tab_cap_edits_within_bounds() {
    let mut runner = runner("mere://welcome");
    let before = runner.state().settings.tab_cap;
    runner.update(Chrome::inc_tab_cap);
    assert_eq!(runner.state().settings.tab_cap, before + 1, "+ raises the cap");
    runner.update(Chrome::dec_tab_cap);
    runner.update(Chrome::dec_tab_cap);
    assert_eq!(runner.state().settings.tab_cap, before - 1, "- lowers it");
    // Lower bound: the cap never drops below 1.
    for _ in 0..200 {
        runner.update(Chrome::dec_tab_cap);
    }
    assert_eq!(runner.state().settings.tab_cap, 1, "cap floors at 1");
    // Upper bound: the cap never exceeds 64.
    for _ in 0..200 {
        runner.update(Chrome::inc_tab_cap);
    }
    assert_eq!(runner.state().settings.tab_cap, 64, "cap ceils at 64");
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

/// A submenu-parent row expands a second panel of its children; keyboard `ArrowRight` focuses
/// the first child, the arrows nav it, and `Enter` picks a child and closes the whole menu.
/// (Nested submenus.)
#[test]
fn submenu_renders_expands_and_picks() {
    use kernel::graph::SemanticSubKind;
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.open_context_menu(
            40.0,
            40.0,
            vec![
                ContextItem::new("Open in splits", ContextAction::OpenSplits),
                ContextItem::with_children(
                    "Relate as\u{2026}",
                    vec![
                        ContextItem::new("Cites", ContextAction::RelateAs(SemanticSubKind::Cites)),
                        ContextItem::new("Quotes", ContextAction::RelateAs(SemanticSubKind::Quotes)),
                    ],
                ),
            ],
        );
    });
    // No child panel until a parent is expanded.
    {
        let dom = runner.dom();
        let dom = dom.borrow();
        assert_eq!(count_class(&dom, runner.root(), "context-submenu"), 0, "no submenu yet");
    }
    // Highlight the parent (index 1) and ArrowRight: the child panel renders, focused on child 0.
    runner.update(|c| {
        c.step_context_menu(1);
        c.step_context_menu(1);
    });
    runner.update(Chrome::enter_submenu);
    {
        let sub = runner.state().context_menu.as_ref().unwrap().submenu.clone();
        assert_eq!(sub.as_ref().map(|s| s.parent), Some(1), "the parent expanded");
        assert_eq!(sub.unwrap().selected, Some(0), "ArrowRight focuses the first child");
        let dom = runner.dom();
        let dom = dom.borrow();
        assert_eq!(count_class(&dom, runner.root(), "context-submenu"), 1, "the child panel renders");
    }
    // Step to the second child and pick it: the kind drains, the whole menu closes.
    runner.update(|c| c.step_context_menu(1));
    runner.update(Chrome::run_context_selection);
    assert_eq!(
        runner.state().pending_context,
        Some(ContextAction::RelateAs(SemanticSubKind::Quotes)),
        "Enter picks the highlighted child"
    );
    assert!(runner.state().context_menu.is_none(), "picking a child closes the whole menu");
}

/// `ArrowLeft` / `Escape` collapse the open submenu one level, keeping the root menu up.
/// (Nested submenus.)
#[test]
fn submenu_collapses_one_level() {
    let mut runner = runner("mere://welcome");
    runner.update(|c| {
        c.open_context_menu(
            40.0,
            40.0,
            vec![ContextItem::with_children(
                "Layout",
                vec![ContextItem::new("Grid", ContextAction::SetLayoutStrategy("grid"))],
            )],
        );
        c.open_submenu(0);
    });
    assert!(runner.state().context_menu.as_ref().unwrap().submenu.is_some());
    // Collapse one level: the submenu closes but the root menu stays open.
    runner.update(Chrome::escape_context_menu);
    assert!(runner.state().context_menu.as_ref().unwrap().submenu.is_none(), "submenu collapsed");
    assert!(runner.state().context_menu.is_some(), "root menu still open");
    // A second collapse closes the whole menu.
    runner.update(Chrome::escape_context_menu);
    assert!(runner.state().context_menu.is_none(), "second Escape closes the menu");
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
    use serval_layout::IncrementalLayout;
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
        let dom = runner.dom();
        let dom = dom.borrow();
        // The production caret primitive (a fresh session's retained layout), the
        // same `IncrementalLayout::caret_rect` the chrome's session overlay uses.
        IncrementalLayout::new(&*dom, TEST_SHEET, 1024.0, 600.0)
            .caret_rect(&*dom, node, byte, 2.0)
            .map(|r| r.x)
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

/// Phase 1 spike (unified-document-host plan, 2026-06-17): one `ServalAppRunner`
/// hosts the real chrome **and** a second pane as two `lens`-composed subtrees of a
/// single shell-container root in one `ScriptedDom`. This is the host-side container
/// that replaces the per-pane-runner fragmentation, proving (a) both surfaces coexist
/// in one document and (b) input routes through the one runner to each surface's own
/// lensed sub-state. The chrome is lifted exactly as it already lifts its omnibar
/// field (`views::chrome_view`); the second pane stands in for roster / apparatus.
#[test]
fn shell_container_hosts_chrome_and_pane_under_one_runner() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use xilem_serval::{el, lens, on_click, AnyView, ServalCtx, ServalElement};

    struct DemoPane {
        clicks: u32,
    }
    struct ShellState {
        chrome: Chrome,
        pane: DemoPane,
    }
    type ShellView = Box<dyn AnyView<ShellState, (), ServalCtx, ServalElement>>;
    type DemoView = Box<dyn AnyView<DemoPane, (), ServalCtx, ServalElement>>;

    fn demo_pane_view(p: &DemoPane) -> DemoView {
        Box::new(on_click(
            el::<_, DemoPane, ()>("section", format!("pane {}", p.clicks)),
            (|p: &mut DemoPane, _: PointerClick| p.clicks += 1) as fn(&mut DemoPane, PointerClick),
        ))
    }

    // The state arg is unused: the lenses pull each surface's sub-state from the
    // runner at build/rebuild time, so the root view is pure structure.
    fn shell_view(_s: &ShellState) -> ShellView {
        let make_chrome: fn(&mut Chrome) -> ChromeView = |c: &mut Chrome| chrome_view(c);
        let to_chrome: fn(&mut ShellState) -> &mut Chrome = |s: &mut ShellState| &mut s.chrome;
        let make_pane: fn(&mut DemoPane) -> DemoView = |p: &mut DemoPane| demo_pane_view(p);
        let to_pane: fn(&mut ShellState) -> &mut DemoPane = |s: &mut ShellState| &mut s.pane;
        Box::new(el::<_, ShellState, ()>(
            "shell",
            (lens(make_chrome, to_chrome), lens(make_pane, to_pane)),
        ))
    }

    let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
    let mut runner = ServalAppRunner::new(
        dom,
        shell_view as fn(&ShellState) -> ShellView,
        ShellState {
            chrome: Chrome::new("mere://welcome"),
            pane: DemoPane { clicks: 0 },
        },
    );

    let root = runner.root();
    let pane_node = {
        let dom = runner.dom();
        let dom = dom.borrow();
        assert!(
            first_tag(&dom, root, "input").is_some(),
            "the chrome subtree (its omnibar input) is in the one document"
        );
        assert!(
            count_tag(&dom, root, "button") >= 2,
            "the chrome's buttons coexist under the shell root"
        );
        first_tag(&dom, root, "section").expect("the pane subtree is under the same shell root")
    };

    // Input dispatched through the single runner reaches the pane's own lensed sub-state.
    runner.dispatch_click(pane_node, PointerClick::at((0.0, 0.0)));
    assert_eq!(
        runner.state().pane.clicks, 1,
        "a click on the pane child mutated its own lensed sub-state, through one runner"
    );
}

/// Phase 2 skeleton spike (orrery-as-element): the orrery renders as a positioned
/// `<div>` holding node cards as `position:absolute; transform: translate(...)` DOM
/// children over an `<external-texture>` underlay (edges + demoted dots, host-painted
/// via gyre), all in one serval document through a runner. Proves the exact view tree
/// the live rework emits; serval's own transform-aware hit-test (serval-layout) covers
/// the picking half, and the cheap-path work covers the RepaintOnly transform motion.
#[test]
fn orrery_element_composes_transform_cards_over_an_external_texture_underlay() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use xilem_serval::{AnyView, ServalCtx, ServalElement, el, external_texture};

    // A few node positions standing in for gyre's per-frame layout output.
    struct OrreryDemo {
        nodes: Vec<(String, f32, f32)>, // (label, world x, world y)
    }
    type OrreryView = Box<dyn AnyView<OrreryDemo, (), ServalCtx, ServalElement>>;

    fn orrery_view(s: &OrreryDemo) -> OrreryView {
        // The scene underlay the host paints (edges, demoted off-screen dots) via gyre.
        let underlay = external_texture::<OrreryDemo, ()>(1, 600, 400);
        // One card per node, placed by a per-node transform (gyre output). The cards
        // are out of flow, so they layer over the in-flow underlay block.
        let cards: Vec<OrreryView> = s
            .nodes
            .iter()
            .map(|(label, x, y)| {
                Box::new(
                    el::<_, OrreryDemo, ()>("div", label.clone())
                        .attr("class", "node-card")
                        .attr(
                            "style",
                            format!("position:absolute;transform:translate({x}px,{y}px)"),
                        ),
                ) as OrreryView
            })
            .collect();
        Box::new(
            el::<_, OrreryDemo, ()>("div", (underlay, cards))
                .attr("class", "orrery")
                .attr("style", "position:relative;width:600px;height:400px"),
        )
    }

    let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
    let runner = ServalAppRunner::new(
        dom,
        orrery_view as fn(&OrreryDemo) -> OrreryView,
        OrreryDemo {
            nodes: vec![
                ("Bird".into(), 120.0, 80.0),
                ("Dog".into(), 300.0, 200.0),
                ("Cat".into(), 60.0, 320.0),
            ],
        },
    );

    let root = runner.root();
    let dom = runner.dom();
    let dom = dom.borrow();
    assert!(
        first_tag(&dom, root, "external-texture").is_some(),
        "the orrery element carries an external-texture underlay for the host scene"
    );
    assert_eq!(
        count_tag(&dom, root, "div"),
        1 + 3,
        "the orrery container plus one transform-positioned card per node"
    );
}
