/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::rc::Rc;

use serval_scripted_dom::ScriptedDom;
use xilem_serval::{
    AnyView, PointerClick, ServalAppRunner, ServalCtx, ServalElement, TextField, TextInput, el,
    lens, memoize, on_click, overlay_at, text_field_typed,
};

use comms::{Direction, ProtocolKind};

use session_runtime::ShellbarEdge;

use super::command::Command;
use super::nav;
use super::suggest;
use super::{Chrome, ContextItem, ContextMenu, HistoryStep, ShellbarPaneStates};

/// The erased view type meerkat's logic produces, so the toolbar's concrete
/// `El<…>` tuple need not be spelled (it grows as the chrome does).
pub type ChromeView = Box<dyn AnyView<Chrome, (), ServalCtx, ServalElement>>;

/// Logic alias for the runner: chrome state → chrome view tree.
pub type ChromeLogic = fn(&Chrome) -> ChromeView;

/// Record a back step for the host to apply to the **focused node's** own history
/// (the chrome can't reach the orrery). The host drains it via `member_history_back`
/// and mirrors the revealed page back into the toolbar/omnibar; the button's
/// enabled-state (host-set from the node) gates an at-the-root click to a no-op.
fn go_back(c: &mut Chrome, _: PointerClick) {
    c.history_step = Some(HistoryStep::Back);
}

/// Record a forward step. Mirror of [`go_back`].
fn go_forward(c: &mut Chrome, _: PointerClick) {
    c.history_step = Some(HistoryStep::Forward);
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
pub(super) fn sync_chrome_from_history(c: &mut Chrome, submitted: bool) {
    let url = c.history.current().to_string();
    c.toolbar.editable.location = url.clone();
    c.toolbar.editable.location_dirty = false;
    c.toolbar.editable.location_submitted = submitted;
    // `can_go_*` is host-driven from the focused node's history (see
    // `Shell::sync_nav_buttons`), not the chrome's suggestions log.
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
    // The nav buttons depend only on their reused capability flag, so memoize
    // each on its bool: typing in the omnibar (which leaves nav state untouched)
    // does not rebuild them. A spent direction carries a `disabled` class (the
    // host sheet greys it; the handler is already a no-op at the history's edge).
    let back = memoize(c.toolbar.can_go_back, |&can_back: &bool| {
        let class = if can_back { "nav" } else { "nav disabled" };
        on_click(
            el::<_, Chrome, ()>("button", "back").attr("class", class),
            go_back as fn(&mut Chrome, PointerClick),
        )
    });
    let forward = memoize(c.toolbar.can_go_forward, |&can_forward: &bool| {
        let class = if can_forward { "nav" } else { "nav disabled" };
        on_click(
            el::<_, Chrome, ()>("button", "forward").attr("class", class),
            go_forward as fn(&mut Chrome, PointerClick),
        )
    });
    // The layout-physics pause/play button: ⏸ while running, ▶ while paused, the
    // same toggle as Space. Memoized on the synced `physics_paused` so the glyph
    // flips with the state; the click sets a one-shot intent the host drains into
    // `orrery.toggle_physics_paused`. (Physics pause.)
    let pause = memoize(c.physics_paused, |&paused: &bool| {
        let glyph = if paused { "\u{25b6}" } else { "\u{23f8}" }; // ▶ / ⏸
        on_click(
            el::<_, Chrome, ()>("button", glyph).attr("class", "nav"),
            (|c: &mut Chrome, _: PointerClick| c.physics_toggle = true) as fn(&mut Chrome, PointerClick),
        )
    });
    // The omnibar text_field, lensed onto `Chrome::omnibar`. `text_field_typed`
    // names its concrete view so the `lens` projection can be a `fn` pointer
    // (no captured borrow), as in pelt-live.
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_omnibar: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.omnibar;
    let omnibar = lens(make, to_omnibar);
    // The p2p sync-status chip (S5.0): an honest one-line readout of the joined
    // lane, sitting at the toolbar's right (the omnibar's flex-grow pushes it
    // there). The host folds the real `SyncStatus` into `c.sync`.
    let sync_chip = el::<_, Chrome, ()>("div", c.sync.summary()).attr("class", "sync-chip");
    // The crawl-progress chip (relational-browse V2): "crawling/crawled: N pages" while
    // a crawl runs and after; hidden when none has run. We toggle a `crawl-chip-hidden`
    // class rather than rely on `:empty` — an empty-string text view still leaves an
    // empty text node child, so `:empty` never matches and the chip would paint as a
    // bare pill while idle.
    let crawl_summary = c.crawl.summary();
    let crawl_class = if crawl_summary.is_empty() { "crawl-chip crawl-chip-hidden" } else { "crawl-chip" };
    let crawl_chip = el::<_, Chrome, ()>("div", crawl_summary).attr("class", crawl_class);
    // The add pill next to the omnibar: a "+" that opens a small menu — Add node /
    // Add tile / Add session — the unified create affordance (the toolbar's old
    // workbench toggle was redundant with the shellbar's, so the pill takes its
    // slot). Static (icon + fixed handler) — memoize on `()` so it is built once.
    // The menu anchors at the click point (`PointerClick::local`, window coords).
    let add_pill = memoize((), |_: &()| {
        on_click(
            el::<_, Chrome, ()>("button", "\u{ff0b}").attr("class", "add-pill"),
            (|c: &mut Chrome, ev: PointerClick| c.open_add_menu(ev.local.0, ev.local.1))
                as fn(&mut Chrome, PointerClick),
        )
    });
    // The branch chip (graphlet wiring Phase 2): a tear-out **branch** window shows the
    // anchor it forked from, so it reads as a distinct grouping, not a plain leaf. Like
    // the crawl chip, it is always present and toggles a `branch-chip-hidden` class (a
    // leaf / the primary leaves `branch_label` `None`), keeping the toolbar tuple arity
    // fixed. (Tear-out gestures G3.)
    let branch_summary = c.branch_label.clone().unwrap_or_default();
    let branch_class = if branch_summary.is_empty() {
        "branch-chip branch-chip-hidden"
    } else {
        "branch-chip"
    };
    let branch_chip = el::<_, Chrome, ()>("div", branch_summary).attr("class", branch_class);
    let toolbar = el::<_, Chrome, ()>(
        "div",
        (back, forward, pause, branch_chip, omnibar, add_pill, sync_chip, crawl_chip),
    )
    .attr("class", "toolbar");

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

    // The chrome tree is a Vec so the optional palette overlay can be appended.
    let mut children: Vec<ChromeView> = vec![Box::new(toolbar), Box::new(suggestions)];
    if c.palette_open {
        children.push(palette_overlay(c));
    }
    if c.find_open {
        children.push(find_bar(c));
    }
    if c.comms.is_open() {
        children.push(comms_pane(c));
    }
    if c.knot_editor_open {
        children.push(knot_editor_pane(c));
    }
    // Shellbar: a pane-toggle strip docked to one window edge (F2.1). Its geometry is
    // set inline each frame by the host via `set_attribute` on the `.shellbar` class
    // node, following the comms-pane pattern. A slim (leaf) window omits it, and the
    // user's hide toggle omits it on a full-chrome window. (MW3 step 4; hide-shellbar.)
    if !c.slim && !c.shellbar_hidden {
        // The shellbar depends only on the pane-open states, so memoize on them:
        // an omnibar keystroke (panes unchanged) skips rebuilding the whole strip.
        children.push(Box::new(memoize(c.shellbar_panes, |panes: &ShellbarPaneStates| {
            shellbar_view(panes)
        })) as ChromeView);
    }
    // The context menu floats over everything (it is a transient cursor pop-up).
    if let Some(menu) = &c.context_menu {
        children.push(context_menu_view(menu));
    }
    // The tear-out drag ghost: a small pill carrying the dragged node's title, floated at
    // the live cursor (the host repositions it each frame in `render`). Pointer-events are
    // off so it never intercepts the drag. (Tear-out gestures, GA-5.)
    if let Some(label) = &c.tear_ghost {
        children.push(Box::new(
            el::<_, Chrome, ()>("div", label.clone()).attr("class", "tear-ghost"),
        ) as ChromeView);
    }
    Box::new(el::<_, Chrome, ()>("div", children).attr("class", "chrome"))
}

/// The right-click context menu: a small panel of action rows floated at the
/// cursor (abs-positioned in window coords). Each row captures its
/// [`ContextAction`] for the host. Rendered in the chrome root over everything.
/// One context-menu row: a submenu parent (`›`, expands its children), a search-result row
/// (label + pin toggle), or a plain leaf (runs its action). `active` marks the keyboard
/// highlight; `open_parent` marks the parent whose submenu is currently expanded (so the render
/// pass can anchor the child panel off this row's rect). (Nested submenus.)
fn menu_item_view(i: usize, item: &ContextItem, active: bool, open_parent: bool) -> ChromeView {
    let base = if active { "context-item-active" } else { "context-item" };
    if item.has_submenu() {
        // Parent row: mouse opens it via the press-gate intercept; this `on_click` serves the
        // keyboard / a11y synthetic-dispatch path. `data-submenu=<i>` + the anchor class let the
        // render pass find this row's rect to place the child panel beside it.
        let class = if open_parent {
            format!("{base} context-submenu-anchor")
        } else {
            base.to_string()
        };
        return Box::new(on_click(
            el::<_, Chrome, ()>("div", format!("{}\u{2002}\u{203a}", item.label))
                .attr("class", class)
                .attr("data-submenu", i.to_string()),
            move |c: &mut Chrome, _: PointerClick| c.open_submenu(i),
        )) as ChromeView;
    }
    let action = item.action;
    match item.pin {
        // A search result (cursor palette): the label runs it, an inline pin toggle pins / unpins
        // it to the curated menu (a ✓ when already pinned). (Searchable context menu S2.)
        Some(pin) => {
            let label = on_click(
                el::<_, Chrome, ()>("div", item.label.clone())
                    .attr("class", base)
                    .attr("style", "flex:1;min-width:0;"),
                move |c: &mut Chrome, _: PointerClick| c.pick_context(action),
            );
            let id = pin.id;
            let (glyph, pin_class) =
                if pin.pinned { ("\u{2713}", "context-pin-on") } else { ("\u{002b}", "context-pin") };
            let pin_btn = on_click(
                el::<_, Chrome, ()>("div", glyph).attr("class", pin_class),
                move |c: &mut Chrome, _: PointerClick| c.pin_from_menu(id),
            );
            let row = el::<_, Chrome, ()>(
                "div",
                vec![Box::new(label) as ChromeView, Box::new(pin_btn) as ChromeView],
            )
            .attr("class", "context-search-row")
            .attr("style", "display: flex; gap: 4px; align-items: stretch;");
            Box::new(row) as ChromeView
        }
        None => {
            let row = on_click(
                el::<_, Chrome, ()>("div", item.label.clone()).attr("class", base),
                move |c: &mut Chrome, _: PointerClick| c.pick_context(action),
            );
            Box::new(row) as ChromeView
        }
    }
}

fn context_menu_view(menu: &ContextMenu) -> ChromeView {
    // The search field (the cursor palette): shows the typed query, or a placeholder when empty.
    // Display-only — the open menu owns the keyboard, so `on_context_menu_key` edits the query.
    let (search_text, search_class) = if menu.query.is_empty() {
        ("Search commands\u{2026}".to_string(), "context-search-empty")
    } else {
        (menu.query.clone(), "context-search")
    };
    let search = el::<_, Chrome, ()>("div", search_text).attr("class", search_class);

    let open_parent = menu.submenu.as_ref().map(|s| s.parent);
    let mut rows: Vec<ChromeView> = vec![Box::new(search) as ChromeView];
    rows.extend(menu.items.iter().enumerate().map(|(i, item)| {
        menu_item_view(i, item, menu.selected == Some(i), open_parent == Some(i))
    }));
    // Point-anchored at the cursor via the overlay primitive (render adds the viewport clamp).
    let root_panel = overlay_at::<_, Chrome, ()>(menu.x, menu.y, rows).attr("class", "context-menu");

    // Depth-1 submenu: a second panel of the open parent's children. The render pass anchors it
    // off the parent row each frame (it starts at the overlay origin). Pushed after the root so
    // serval's stacking paints it over the root panel. (Nested submenus.)
    let mut layer: Vec<ChromeView> = vec![Box::new(root_panel) as ChromeView];
    if let Some(sub) = &menu.submenu {
        if let Some(parent) = menu.items.get(sub.parent) {
            // Child rows are leaves: they run their action and carry their own active class (so the
            // root menu's scroll-into-view never latches onto a child rect), and never emit
            // `data-submenu` (so a stray nested parent can't shadow a root index in the press
            // hit-test). (Nested submenus.)
            let child_rows: Vec<ChromeView> = parent
                .children
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let action = item.action;
                    let class = if sub.selected == Some(i) {
                        "context-subitem-active"
                    } else {
                        "context-item"
                    };
                    Box::new(on_click(
                        el::<_, Chrome, ()>("div", item.label.clone()).attr("class", class),
                        move |c: &mut Chrome, _: PointerClick| c.pick_context(action),
                    )) as ChromeView
                })
                .collect();
            // Built at the overlay origin; the render pass anchors it off the parent row via
            // `anchor_point` each frame (overwriting this placeholder before paint). (Submenus.)
            let sub_panel =
                overlay_at::<_, Chrome, ()>(0.0, 0.0, child_rows).attr("class", "context-submenu");
            layer.push(Box::new(sub_panel) as ChromeView);
        }
    }
    Box::new(el::<_, Chrome, ()>("div", layer).attr("class", "context-menu-layer"))
}

/// The command-palette overlay: a centered panel with the query field and the
/// filtered command rows (the highlight carrying a distinct class). Clicking the
/// backdrop closes it; clicking a row runs that command. Rendered into the
/// chrome root, so the host composites it over the content (like the dropdown).
fn palette_overlay(c: &Chrome) -> ChromeView {
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_input: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.palette_input;
    let input = lens(make, to_input);

    let rows: Vec<ChromeView> = c
        .palette_items()
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            let class = if c.palette.selected_index == Some(i) {
                "cmd-row-active"
            } else {
                "cmd-row"
            };
            let row = on_click(
                el::<_, Chrome, ()>("div", item.label()).attr("class", class),
                move |c: &mut Chrome, _: PointerClick| c.run_palette_item_and_close(item),
            );
            Box::new(row) as ChromeView
        })
        .collect();
    let list = el::<_, Chrome, ()>("div", rows).attr("class", "cmd-list");
    let panel = el::<_, Chrome, ()>("div", (input, list)).attr("class", "palette");
    // The backdrop closes the palette on a click that misses the panel (a click
    // on a row runs the command first, then bubbles here — close is idempotent).
    let overlay = on_click(
        el::<_, Chrome, ()>("div", panel).attr("class", "palette-overlay"),
        |c: &mut Chrome, _: PointerClick| c.close_palette(),
    );
    Box::new(overlay)
}

/// The find-in-page bar (Ctrl+F): a query field docked top-right under the
/// toolbar. The host pushes the query to the content actor on each edit and
/// composites the match highlights over the page (HTML lane); this is just the
/// query surface. Mirrors `palette_overlay`'s field-via-lens construction.
fn find_bar(c: &Chrome) -> ChromeView {
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_input: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.find_input;
    let input = lens(make, to_input);
    let label = el::<_, Chrome, ()>("div", "Find").attr("class", "find-label");
    // "active/total" once a query is present; "0/0" when nothing matched. The
    // count is synced host-side from the constellation each frame (`find_count`).
    let count_text = if c.find_input.text().is_empty() {
        String::new()
    } else if c.find_count == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", c.find_active.min(c.find_count - 1) + 1, c.find_count)
    };
    let count = el::<_, Chrome, ()>("div", count_text).attr("class", "find-count");
    let panel = el::<_, Chrome, ()>("div", (label, input, count)).attr("class", "find-bar");
    Box::new(el::<_, Chrome, ()>("div", panel).attr("class", "find-overlay"))
}

/// The docked knot editor: a source field (a `text_field` over the knot buffer) in
/// a panel, mirroring the comms pane's structure. Highlighting and the rendered
/// preview layer on in later slices.
fn knot_editor_pane(_c: &Chrome) -> ChromeView {
    let title_text =
        el::<_, Chrome, ()>("div", "Editor").attr("class", "knot-editor-title-text");
    let close_x = on_click(
        el::<_, Chrome, ()>("button", "\u{00d7}").attr("class", "knot-editor-btn"),
        |c: &mut Chrome, _: PointerClick| c.close_knot_editor(),
    );
    let header =
        el::<_, Chrome, ()>("div", (title_text, close_x)).attr("class", "knot-editor-title");

    // The source field: a `text_field` lensed onto the knot buffer, exactly the
    // comms-draft pattern. Its class is the focus key (see ime.rs / input.rs).
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let to_source: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.knot_source;
    let field = lens(make, to_source);
    let source = el::<_, Chrome, ()>("div", field)
        .attr("class", "knot-editor-source")
        .attr(
            "style",
            "display: block; min-height: 360px; font-family: monospace; white-space: pre-wrap;",
        );

    Box::new(
        el::<_, Chrome, ()>("div", (header, source))
            .attr("class", "knot-editor-pane")
            .attr(
                "style",
                "position: absolute; left: 160px; top: 72px; width: 640px; height: 520px; \
                 background: #1b1b1f; color: #e6e6e6; border: 1px solid #3a3a42; \
                 border-radius: 6px; padding: 12px; overflow: auto; z-index: 90;",
            ),
    )
}

/// The docked comms pane (P6): a right-edge panel of conversations, an open
/// thread, and a compose field. Rendered into the chrome root and composited over
/// the content like the settings panel; mirrors its structure. Placeholder data
/// for now — the live misfin / murm adapters fill `c.comms` through the event loop
/// in a later slice.
fn comms_pane(c: &Chrome) -> ChromeView {
    let mut children: Vec<ChromeView> = Vec::new();

    // Header: title + close.
    let title_text = el::<_, Chrome, ()>("div", "Comms").attr("class", "comms-title-text");
    let close_x = on_click(
        el::<_, Chrome, ()>("button", "\u{00d7}").attr("class", "comms-btn"),
        |c: &mut Chrome, _: PointerClick| c.close_comms(),
    );
    children.push(Box::new(
        el::<_, Chrome, ()>("div", (title_text, close_x)).attr("class", "comms-title"),
    ));

    // Surfaced backend failures (never hidden) — empty for the placeholder phase.
    for failure in &c.comms.failures {
        let line = el::<_, Chrome, ()>("div", format!("{:?}: {}", failure.protocol, failure.error))
            .attr("class", "comms-failure");
        children.push(Box::new(line));
    }

    if c.comms.selected().is_some() {
        // Thread view: back to the list, the conversation title, its messages, and
        // a compose row.
        let back = on_click(
            el::<_, Chrome, ()>("button", "\u{2039} Conversations").attr("class", "comms-back"),
            |c: &mut Chrome, _: PointerClick| c.comms.clear_selection(),
        );
        children.push(Box::new(back));

        if let Some(conversation) = c.comms.selected_conversation() {
            children.push(Box::new(
                el::<_, Chrome, ()>("div", conversation.title.clone())
                    .attr("class", "comms-thread-title"),
            ));
        }

        for message in &c.comms.thread {
            let class = match message.direction {
                Direction::Outgoing => "comms-msg-out",
                Direction::Incoming => "comms-msg-in",
            };
            let mut text = String::new();
            if let Some(subject) = &message.subject {
                text.push_str(subject);
                text.push('\n');
            }
            text.push_str(message.body.text());
            children.push(Box::new(
                el::<_, Chrome, ()>("div", text).attr("class", class),
            ));

            // A received cabal invite (a ticket in the body) gets a Join button.
            if message.direction == Direction::Incoming {
                if let Some(ticket) = cabal_ticket_in(message.body.text()) {
                    let join = on_click(
                        el::<_, Chrome, ()>("button", "Join this cabal")
                            .attr("class", "comms-new-btn"),
                        move |c: &mut Chrome, _: PointerClick| c.connect_cabal(ticket.clone()),
                    );
                    children.push(Box::new(join));
                }
            }
        }

        // Compose: a text field lensed onto `comms_draft` + a Send button.
        let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
        let to_draft: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.comms_draft;
        let field = lens(make, to_draft);
        let send = on_click(
            el::<_, Chrome, ()>("button", "Send").attr("class", "comms-send"),
            |c: &mut Chrome, _: PointerClick| c.send_comms(),
        );
        children.push(Box::new(
            el::<_, Chrome, ()>("div", (field, send)).attr("class", "comms-compose"),
        ));

        // The last send's outcome (a misfin status, or a failure reason), shown
        // under the compose box — the feedback for sends with no in-thread echo.
        if let Some(status) = &c.comms.send_status {
            children.push(Box::new(
                el::<_, Chrome, ()>("div", status.clone()).attr("class", "comms-status"),
            ));
        }
    } else if let Some(form) = c.comms.new_message.as_ref() {
        children.push(new_message_form(form));
        // The send outcome (delivered / a misfin status / a failure reason), shown
        // under the still-open form so a new-message send isn't silent.
        if let Some(status) = &c.comms.send_status {
            children.push(Box::new(
                el::<_, Chrome, ()>("div", status.clone()).attr("class", "comms-status"),
            ));
        }
    } else {
        // The conversation list, led by the compose / invite actions + connect info.
        let new_btn = on_click(
            el::<_, Chrome, ()>("button", "+ New message").attr("class", "comms-new-btn"),
            |c: &mut Chrome, _: PointerClick| c.open_new_message(),
        );
        children.push(Box::new(new_btn));
        if c.comms.cabal_ticket.is_some() {
            // Mails the cabal join ticket to a peer (pre-fills a misfin message).
            let share = on_click(
                el::<_, Chrome, ()>("button", "Share cabal invite").attr("class", "comms-new-btn"),
                |c: &mut Chrome, _: PointerClick| c.share_cabal_invite(),
            );
            children.push(Box::new(share));
        }
        if let Some(address) = &c.comms.misfin_address {
            children.push(Box::new(
                el::<_, Chrome, ()>("div", format!("Your address: {address}"))
                    .attr("class", "comms-field-label"),
            ));
        }
        for conversation in &c.comms.inbox {
            let id = conversation.id.clone();
            let label = if conversation.unread > 0 {
                format!("{}  ({})", conversation.title, conversation.unread)
            } else {
                conversation.title.clone()
            };
            let row = on_click(
                el::<_, Chrome, ()>("div", label).attr("class", "comms-row"),
                move |c: &mut Chrome, _: PointerClick| c.select_conversation(id.clone()),
            );
            children.push(Box::new(row));
        }
        if c.comms.inbox.is_empty() {
            children.push(Box::new(
                el::<_, Chrome, ()>("div", "No conversations yet.").attr("class", "comms-empty"),
            ));
        }
    }

    Box::new(el::<_, Chrome, ()>("div", children).attr("class", "comms-pane"))
}

/// Extract a cabal join ticket (an iroh endpoint token) from a message body, if
/// one is present — the body of a "Share cabal invite" message.
fn cabal_ticket_in(body: &str) -> Option<String> {
    body.split_whitespace()
        .find(|token| token.starts_with("endpoint") && token.len() > 32)
        .map(str::to_string)
}

/// The compose-new form: a protocol toggle (Misfin / Cable), a recipient field
/// (misfin only — Cable targets the cabal), a body field + Send, and Cancel.
fn new_message_form(form: &comms::NewMessageForm) -> ChromeView {
    let mut rows: Vec<ChromeView> = Vec::new();

    // Title + cancel.
    let title = el::<_, Chrome, ()>("div", "New message").attr("class", "comms-thread-title");
    let cancel = on_click(
        el::<_, Chrome, ()>("button", "Cancel").attr("class", "comms-btn"),
        |c: &mut Chrome, _: PointerClick| c.close_new_message(),
    );
    rows.push(Box::new(
        el::<_, Chrome, ()>("div", (title, cancel)).attr("class", "comms-title"),
    ));

    // Protocol toggle: the active option carries a distinct class.
    let proto_class = |active: bool| {
        if active {
            "comms-proto-active"
        } else {
            "comms-proto"
        }
    };
    let misfin_btn = on_click(
        el::<_, Chrome, ()>("button", "Misfin")
            .attr("class", proto_class(form.protocol == ProtocolKind::Misfin)),
        |c: &mut Chrome, _: PointerClick| c.set_new_message_protocol(ProtocolKind::Misfin),
    );
    let cable_btn = on_click(
        el::<_, Chrome, ()>("button", "Cable")
            .attr("class", proto_class(form.protocol == ProtocolKind::Murm)),
        |c: &mut Chrome, _: PointerClick| c.set_new_message_protocol(ProtocolKind::Murm),
    );
    rows.push(Box::new(
        el::<_, Chrome, ()>("div", (misfin_btn, cable_btn)).attr("class", "comms-proto-row"),
    ));

    // Recipient: misfin needs a server address; Cable targets the cabal. A small
    // subheading labels each field (serval's input has no placeholder ghost text).
    if form.protocol == ProtocolKind::Misfin {
        rows.push(Box::new(
            el::<_, Chrome, ()>("div", "To — a misfin address (mailbox@server)")
                .attr("class", "comms-field-label"),
        ));
        let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
        let to_lens: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.comms_new_to;
        rows.push(Box::new(
            el::<_, Chrome, ()>("div", lens(make, to_lens)).attr("class", "comms-new-to"),
        ));
    } else {
        rows.push(Box::new(
            el::<_, Chrome, ()>("div", "To — the Project cabal").attr("class", "comms-field-label"),
        ));
    }

    // Body + Send.
    rows.push(Box::new(
        el::<_, Chrome, ()>("div", "Message").attr("class", "comms-field-label"),
    ));
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let body_lens: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.comms_new_body;
    let send = on_click(
        el::<_, Chrome, ()>("button", "Send").attr("class", "comms-send"),
        |c: &mut Chrome, _: PointerClick| c.send_new_message(),
    );
    rows.push(Box::new(
        el::<_, Chrome, ()>("div", (lens(make, body_lens), send)).attr("class", "comms-new-body"),
    ));

    Box::new(el::<_, Chrome, ()>("div", rows).attr("class", "comms-new"))
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
    c.content_location = url.clone();
    c.history.visit(url);
    sync_chrome_from_history(c, true);
}

/// The shellbar strip: one pane-toggle button per toggle-able pane, each active
/// state mirroring the current frame layout. The host positions the div inline
/// each frame via the `.shellbar` class node, following the comms-pane geometry
/// pattern (F2.1).
fn shellbar_view(panes: &ShellbarPaneStates) -> ChromeView {
    fn btn(label: &'static str, active: bool, cmd: Command) -> ChromeView {
        let class = if active { "shellbar-btn-active" } else { "shellbar-btn" };
        Box::new(on_click(
            el::<_, Chrome, ()>("button", label).attr("class", class),
            move |c: &mut Chrome, _: PointerClick| c.run_command(cmd),
        )) as ChromeView
    }
    let buttons: Vec<ChromeView> = vec![
        btn("\u{229e}", panes.workbench, Command::ToggleWorkbench), // ⊞
        btn("\u{2261}", panes.roster, Command::ToggleRoster),       // ≡
        btn("\u{25ce}", panes.gloss, Command::ToggleGloss),         // ◎
        btn("\u{21dd}", panes.trail, Command::ToggleTrail),         // ⇝
        btn("\u{2697}", panes.alembic, Command::ToggleAlembic),     // ⚗
        btn("\u{2699}", panes.apparatus, Command::ToggleApparatus), // ⚙
        btn("\u{25c9}", panes.inspector, Command::ToggleInspector), // ◉
        btn("\u{2692}", panes.steward, Command::ToggleSteward),     // ⚒
        btn("\u{2709}", panes.comms, Command::ToggleComms),         // ✉
    ];
    Box::new(el::<_, Chrome, ()>("div", buttons).attr("class", "shellbar"))
}

/// Build the chrome via a [`ServalAppRunner`] over a fresh [`ScriptedDom`] — the
/// same diff path the windowed host will drive, minus layout / paint. Returns the
/// runner so callers (and tests) can inspect the DOM, dispatch input, and rebuild.
pub fn runner(initial_location: &str) -> ServalAppRunner<Chrome, ChromeLogic, ChromeView> {
    let dom: Rc<RefCell<ScriptedDom>> = Rc::new(RefCell::new(ScriptedDom::new()));
    ServalAppRunner::new(
        dom,
        chrome_view as ChromeLogic,
        Chrome::new(initial_location),
    )
}
