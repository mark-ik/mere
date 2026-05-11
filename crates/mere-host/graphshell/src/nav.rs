/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Nav-button row aspect of the graphshell chrome.
//!
//! Renders back / forward / refresh affordances. Disabled state comes
//! from `ToolbarViewModel::can_go_back` / `can_go_forward`. The host
//! provides the click handlers (typically built with `cx.listener` so
//! they call back into the host's `navigate_to` / `go_back` /
//! `go_forward` / `reload` methods).

use gpui::{
    AnyElement, App, MouseButton, MouseUpEvent, Window, CursorStyle, div, prelude::*, px, rgb,
};
use mere_graphshell::frame_model::ToolbarViewModel;

pub fn render(
    toolbar: &ToolbarViewModel,
    on_back: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_forward: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_reload: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap_1()
        .child(button_glyph("back", "←", toolbar.can_go_back, on_back))
        .child(button_glyph(
            "forward",
            "→",
            toolbar.can_go_forward,
            on_forward,
        ))
        .child(button_glyph("reload", "↻", true, on_reload))
        .into_any_element()
}

fn button_glyph(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    on_click: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let bg = if enabled { rgb(0x2a2a2a) } else { rgb(0x1a1a1a) };
    let fg = if enabled { rgb(0xd0d0d0) } else { rgb(0x505050) };
    let mut button = div()
        .id(gpui::SharedString::from(format!("nav-button-{id}")))
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(fg)
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x3a3a3a))
        .child(label);
    if enabled {
        button = button
            .cursor(CursorStyle::PointingHand)
            .hover(|s| s.bg(rgb(0x383838)).border_color(rgb(0x555555)))
            .on_mouse_up(MouseButton::Left, on_click);
    }
    button.into_any_element()
}
