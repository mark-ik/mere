/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::RefCell;
use std::rc::Rc;

use serval_scripted_dom::ScriptedDom;
use xilem_serval::{
    AnyView, PointerClick, ServalAppRunner, ServalCtx, ServalElement, TextField, TextInput, el,
    lens, on_click, text_field_typed,
};

use comms::{Direction, ProtocolKind};

use session_runtime::ShellbarEdge;

use super::command::Command;
use super::nav;
use super::suggest;
use super::{Chrome, ContextMenu, HistoryStep, ShellbarPaneStates};

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
    // `App::sync_nav_buttons`), not the chrome's suggestions log.
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
    let back_class = if c.toolbar.can_go_back {
        "nav"
    } else {
        "nav disabled"
    };
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
    // The p2p sync-status chip (S5.0): an honest one-line readout of the joined
    // lane, sitting at the toolbar's right (the omnibar's flex-grow pushes it
    // there). The host folds the real `SyncStatus` into `c.sync`.
    let sync_chip = el::<_, Chrome, ()>("div", c.sync.summary()).attr("class", "sync-chip");
    // A workbench toggle next to the omnibar — the reliable way to flip the tiled
    // view (Ctrl+T can be flaky), routed through the `ToggleWorkbench` host action.
    let workbench_btn = on_click(
        el::<_, Chrome, ()>("button", "\u{229e}").attr("class", "workbench-btn"),
        |c: &mut Chrome, _: PointerClick| c.run_command(Command::ToggleWorkbench),
    );
    let toolbar = el::<_, Chrome, ()>("div", (back, forward, omnibar, workbench_btn, sync_chip))
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
    if c.settings_open {
        children.push(settings_overlay(c));
    }
    if c.comms.is_open() {
        children.push(comms_pane(c));
    }
    // Shellbar: always-visible pane-toggle strip docked to one window edge (F2.1).
    // Its geometry is set inline each frame by the host via `set_attribute` on the
    // `.shellbar` class node, following the comms-pane pattern.
    children.push(shellbar_view(&c.shellbar_panes));
    // The context menu floats over everything (it is a transient cursor pop-up).
    if let Some(menu) = &c.context_menu {
        children.push(context_menu_view(menu));
    }
    Box::new(el::<_, Chrome, ()>("div", children).attr("class", "chrome"))
}

/// The right-click context menu: a small panel of action rows floated at the
/// cursor (abs-positioned in window coords). Each row captures its
/// [`ContextAction`] for the host. Rendered in the chrome root over everything.
fn context_menu_view(menu: &ContextMenu) -> ChromeView {
    let rows: Vec<ChromeView> = menu
        .items
        .iter()
        .map(|item| {
            let action = item.action;
            let row = on_click(
                el::<_, Chrome, ()>("div", item.label.clone()).attr("class", "context-item"),
                move |c: &mut Chrome, _: PointerClick| c.pick_context(action),
            );
            Box::new(row) as ChromeView
        })
        .collect();
    let panel = el::<_, Chrome, ()>("div", rows)
        .attr("class", "context-menu")
        .attr(
            "style",
            format!("position: absolute; left: {}px; top: {}px;", menu.x, menu.y),
        );
    Box::new(panel)
}

/// The settings overlay: a centered panel of controls (just the active-tab cap
/// for now — a value flanked by − / + buttons). Clicking the backdrop closes it;
/// the buttons edit [`Chrome::settings`], which the host applies + persists.
/// Rendered into the chrome root, composited over the content like the palette.
fn settings_overlay(c: &Chrome) -> ChromeView {
    let dec = on_click(
        el::<_, Chrome, ()>("button", "\u{2212}").attr("class", "set-btn"),
        |c: &mut Chrome, _: PointerClick| c.dec_tab_cap(),
    );
    let value = el::<_, Chrome, ()>("div", format!("Active tab cap: {}", c.settings.tab_cap))
        .attr("class", "set-value");
    let inc = on_click(
        el::<_, Chrome, ()>("button", "+").attr("class", "set-btn"),
        |c: &mut Chrome, _: PointerClick| c.inc_tab_cap(),
    );
    let cap_row = el::<_, Chrome, ()>("div", (dec, value, inc)).attr("class", "set-row");
    let title_text = el::<_, Chrome, ()>("div", "Settings").attr("class", "set-title-text");
    let close_x = on_click(
        el::<_, Chrome, ()>("button", "\u{00d7}").attr("class", "set-btn"),
        |c: &mut Chrome, _: PointerClick| c.close_settings(),
    );
    let title = el::<_, Chrome, ()>("div", (title_text, close_x)).attr("class", "set-title");
    let panel = el::<_, Chrome, ()>("div", (title, cap_row)).attr("class", "settings");
    // No backdrop-close: a click inside the panel must not bubble to a close, so
    // the overlay is just a centering container — close via the × button or Escape.
    let overlay = el::<_, Chrome, ()>("div", panel).attr("class", "settings-overlay");
    Box::new(overlay)
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
        .palette_commands()
        .into_iter()
        .enumerate()
        .map(|(i, cmd)| {
            let class = if c.palette.selected_index == Some(i) {
                "cmd-row-active"
            } else {
                "cmd-row"
            };
            let row = on_click(
                el::<_, Chrome, ()>("div", cmd.label()).attr("class", class),
                move |c: &mut Chrome, _: PointerClick| c.run_command_and_close(cmd),
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

/// The shellbar strip: five pane-toggle buttons whose active state mirrors the
/// current frame layout. The host positions the div inline each frame via the
/// `.shellbar` class node, following the comms-pane geometry pattern (F2.1).
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
        btn("\u{2699}", panes.apparatus, Command::ToggleApparatus), // ⚙
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
