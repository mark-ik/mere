/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Chrome panel views: context menu rows, palette/find/knot/comms overlays.

use super::*;

/// One context-menu row: a submenu parent (`›`, expands its children), a search-result row
/// (label + pin toggle), or a plain leaf (runs its action). `active` marks the keyboard
/// highlight; `open_parent` marks the parent whose submenu is currently expanded (so the render
/// pass can anchor the child panel off this row's rect). (Nested submenus.)
pub(crate) fn menu_item_view(
    i: usize,
    item: &ContextItem,
    active: bool,
    open_parent: bool,
) -> ChromeView {
    let base = if active {
        "context-item-active"
    } else {
        "context-item"
    };
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
            let (glyph, pin_class) = if pin.pinned {
                ("\u{2713}", "context-pin-on")
            } else {
                ("\u{002b}", "context-pin")
            };
            let pin_btn = on_click(
                el::<_, Chrome, ()>("div", glyph).attr("class", pin_class),
                move |c: &mut Chrome, _: PointerClick| c.pin_from_menu(id),
            );
            let row = el::<_, Chrome, ()>(
                "div",
                vec![
                    Box::new(label) as ChromeView,
                    Box::new(pin_btn) as ChromeView,
                ],
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

pub(crate) fn context_menu_view(menu: &ContextMenu) -> ChromeView {
    // The search field (the cursor palette): shows the typed query, or a placeholder when empty.
    // Display-only — the open menu owns the keyboard, so `on_context_menu_key` edits the query.
    let (search_text, search_class) = if menu.query.is_empty() {
        (
            "Search commands\u{2026}".to_string(),
            "context-search-empty",
        )
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
    let root_panel =
        overlay_at::<_, Chrome, ()>(menu.x, menu.y, rows).attr("class", "context-menu");

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
pub(crate) fn palette_overlay(c: &Chrome) -> ChromeView {
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
pub(crate) fn find_bar(c: &Chrome) -> ChromeView {
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
        format!(
            "{}/{}",
            c.find_active.min(c.find_count - 1) + 1,
            c.find_count
        )
    };
    let count = el::<_, Chrome, ()>("div", count_text).attr("class", "find-count");
    let panel = el::<_, Chrome, ()>("div", (label, input, count)).attr("class", "find-bar");
    Box::new(el::<_, Chrome, ()>("div", panel).attr("class", "find-overlay"))
}

/// The docked knot editor: a source field (a `text_field` over the knot buffer) in
/// a panel, mirroring the comms pane's structure. Highlighting and the rendered
/// preview layer on in later slices.
pub(crate) fn knot_editor_pane(c: &Chrome) -> ChromeView {
    let title = if c.knot_editor_label.is_empty() {
        "Editor".to_string()
    } else {
        c.knot_editor_label.clone()
    };
    let title_text = el::<_, Chrome, ()>("div", title).attr("class", "knot-editor-title-text");
    // The action buttons: an optional preview toggle (flips the opaque source overlay off,
    // revealing the live-rendered note tile behind — only bound notes have a tile, so it is
    // hidden without a target; the label names the view you switch *to*), then Save + close.
    // (Phase 2 toggle.)
    let mut actions_children: Vec<ChromeView> = Vec::new();
    if c.knot_target.is_some() {
        let toggle_label = if c.knot_editor_preview {
            "Edit"
        } else {
            "Preview"
        };
        actions_children.push(Box::new(button(
            toggle_label,
            "knot-editor-btn knot-editor-preview",
            |c: &mut Chrome, _: PointerClick| c.toggle_knot_editor_preview(),
        )));
    }
    actions_children.push(Box::new(button(
        "Save",
        "knot-editor-btn knot-editor-save",
        |c: &mut Chrome, _: PointerClick| c.request_knot_editor_save(),
    )));
    actions_children.push(Box::new(button(
        "\u{00d7}",
        "knot-editor-btn knot-editor-close",
        |c: &mut Chrome, _: PointerClick| c.request_knot_editor_close(),
    )));
    let actions = el::<_, Chrome, ()>("div", actions_children).attr("class", "knot-editor-actions");
    let header =
        el::<_, Chrome, ()>("div", (title_text, actions)).attr("class", "knot-editor-title");

    let (x0, y0, w, h) = match c.knot_editor_rect {
        Some([x0, y0, x1, y1]) => (x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)),
        None => (160.0, 72.0, 640.0, 520.0),
    };

    if c.knot_editor_preview {
        // Preview: a compact transparent header strip only, so the live-rendered note tile
        // behind (rendered from the same buffer, see render/cards.rs) shows through and stays
        // scrollable — the pane's laid-out rect shrinks to the header, so `knot_editor_pane_at`
        // routes only header clicks to chrome and lets tile presses fall through.
        let style = format!(
            "position: absolute; left: {x0}px; top: {y0}px; width: {w}px; \
             box-sizing: border-box; z-index: 90;"
        );
        return Box::new(
            el::<_, Chrome, ()>("div", header)
                .attr("class", "knot-editor-pane knot-editor-preview-mode")
                .attr("style", style),
        );
    }

    // Edit: the opaque source field over the tile. A `text_field` lensed onto the knot
    // buffer (the comms-draft pattern); its class is the focus key (see ime.rs / input.rs).
    // A multi-line styled textarea (Enter inserts a newline, Up/Down move between lines),
    // with illume's highlight + entity spans painted as `syntax-*` classes tinct colours.
    let make: fn(&mut TextInput) -> TextField =
        |t: &mut TextInput| xilem_serval::highlighted_textarea(t, xilem_serval::Highlight::Note);
    let to_source: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.knot_source;
    let field = lens(make, to_source);
    let source = el::<_, Chrome, ()>("div", field)
        .attr("class", "knot-editor-source")
        .attr(
            "style",
            "display: block; min-height: 360px; font-family: monospace; white-space: pre-wrap;",
        );
    let style = format!(
        "position: absolute; left: {x0}px; top: {y0}px; width: {w}px; height: {h}px; \
         box-sizing: border-box; background: #1b1b1f; color: #e6e6e6; \
         border: 1px solid #3a3a42; border-radius: 6px; padding: 12px; \
         overflow: auto; z-index: 90;"
    );
    Box::new(
        el::<_, Chrome, ()>("div", (header, source))
            .attr("class", "knot-editor-pane")
            .attr("style", style),
    )
}

/// The docked comms pane (P6): a right-edge panel of conversations, an open
/// thread, and a compose field. Rendered into the chrome root and composited over
/// the content like the settings panel; mirrors its structure. Placeholder data
/// for now — the live misfin / murm adapters fill `c.comms` through the event loop
/// in a later slice.
pub(crate) fn comms_pane(c: &Chrome) -> ChromeView {
    let mut children: Vec<ChromeView> = Vec::new();

    // Header: title + close.
    let title_text = el::<_, Chrome, ()>("div", "Comms").attr("class", "comms-title-text");
    let close_x = button(
        "\u{00d7}",
        "comms-btn",
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
        let back = button(
            "\u{2039} Conversations",
            "comms-back",
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
                    let join = button(
                        "Join this murmur",
                        "comms-new-btn",
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
        let send = button("Send", "comms-send", |c: &mut Chrome, _: PointerClick| {
            c.send_comms()
        });
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
        let new_btn = button(
            "+ New message",
            "comms-new-btn",
            |c: &mut Chrome, _: PointerClick| c.open_new_message(),
        );
        children.push(Box::new(new_btn));
        if c.comms.cabal_ticket.is_some() {
            // Mails the cabal join ticket to a peer (pre-fills a misfin message).
            let share = button(
                "Share murmur invite",
                "comms-new-btn",
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
pub(crate) fn cabal_ticket_in(body: &str) -> Option<String> {
    body.split_whitespace()
        .find(|token| token.starts_with("endpoint") && token.len() > 32)
        .map(str::to_string)
}

/// The compose-new form: a protocol toggle (Misfin / Cable), a recipient field
/// (misfin only — Cable targets the cabal), a body field + Send, and Cancel.
pub(crate) fn new_message_form(form: &comms::NewMessageForm) -> ChromeView {
    let mut rows: Vec<ChromeView> = Vec::new();

    // Title + cancel.
    let title = el::<_, Chrome, ()>("div", "New message").attr("class", "comms-thread-title");
    let cancel = button("Cancel", "comms-btn", |c: &mut Chrome, _: PointerClick| {
        c.close_new_message()
    });
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
    let misfin_btn = button(
        "Misfin",
        proto_class(form.protocol == ProtocolKind::Misfin),
        |c: &mut Chrome, _: PointerClick| c.set_new_message_protocol(ProtocolKind::Misfin),
    );
    let cable_btn = button(
        "Cable",
        proto_class(form.protocol == ProtocolKind::Murm),
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
            el::<_, Chrome, ()>("div", "To — the Project murmur").attr("class", "comms-field-label"),
        ));
    }

    // Body + Send.
    rows.push(Box::new(
        el::<_, Chrome, ()>("div", "Message").attr("class", "comms-field-label"),
    ));
    let make: fn(&mut TextInput) -> TextField = |t: &mut TextInput| text_field_typed(t);
    let body_lens: fn(&mut Chrome) -> &mut TextInput = |c: &mut Chrome| &mut c.comms_new_body;
    let send = button("Send", "comms-send", |c: &mut Chrome, _: PointerClick| {
        c.send_new_message()
    });
    rows.push(Box::new(
        el::<_, Chrome, ()>("div", (lens(make, body_lens), send)).attr("class", "comms-new-body"),
    ));

    Box::new(el::<_, Chrome, ()>("div", rows).attr("class", "comms-new"))
}
