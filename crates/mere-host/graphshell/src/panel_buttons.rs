/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Panel summon button group for the shellbar.
//!
//! Toggle buttons for workbench / gloss / apparatus + a "summon
//! orrery for new graph" button. Each button takes a click handler
//! that the host wires to its panel-summon methods. Active state
//! (whether a panel of the kind is currently in the layout) is
//! piped in so the buttons render highlighted.

use gpui::{
    AnyElement, App, CursorStyle, MouseButton, MouseUpEvent, Window, div, prelude::*, px, rgb,
};

/// Layout state for the buttons — which kinds are currently present
/// in the frame, so toggle buttons can render with an "active"
/// highlight when their kind is showing.
#[derive(Clone, Copy, Debug, Default)]
pub struct PanelButtonState {
    pub workbench_summoned: bool,
    pub gloss_summoned: bool,
    pub apparatus_summoned: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    state: PanelButtonState,
    on_toggle_workbench: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_gloss: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_apparatus: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_summon_orrery_new_graph: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_graph_switcher: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    graph_switcher_open: bool,
) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .gap_1()
        .child(toggle_button(
            "panel-workbench",
            "W",
            "Workbench",
            state.workbench_summoned,
            on_toggle_workbench,
        ))
        .child(toggle_button(
            "panel-gloss",
            "G",
            "Gloss",
            state.gloss_summoned,
            on_toggle_gloss,
        ))
        .child(toggle_button(
            "panel-apparatus",
            "A",
            "Apparatus",
            state.apparatus_summoned,
            on_toggle_apparatus,
        ))
        .child(plus_button(
            "panel-new-orrery",
            "+◯",
            "Summon orrery for new graph",
            on_summon_orrery_new_graph,
        ))
        .child(toggle_button(
            "panel-graphs",
            "≡",
            "Switch graph",
            graph_switcher_open,
            on_toggle_graph_switcher,
        ))
        .into_any_element()
}

/// Two-state toggle button: highlights when `active`, plain otherwise.
fn toggle_button(
    id: &'static str,
    glyph: &'static str,
    tooltip: &'static str,
    active: bool,
    on_click: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let (bg, fg, border) = if active {
        (rgb(0x2d4870), rgb(0xffffff), rgb(0x5680c0))
    } else {
        (rgb(0x2a2a2a), rgb(0xc0c0c0), rgb(0x3a3a3a))
    };
    div()
        .id(gpui::SharedString::from(id))
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(fg)
        .rounded_sm()
        .border_1()
        .border_color(border)
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x3a4060)).border_color(rgb(0x6090d0)))
        .on_mouse_up(MouseButton::Left, on_click)
        .child(div().text_xs().child(glyph))
        // Tooltip is not yet wired into a real popover; the title
        // attribute is the placeholder until that lands.
        .id(gpui::SharedString::from(format!("{id}-tt-{tooltip}")))
        .into_any_element()
}

/// Action button — always non-active, used for "summon orrery for
/// new graph" and similar one-shot operations.
fn plus_button(
    id: &'static str,
    glyph: &'static str,
    _tooltip: &'static str,
    on_click: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(gpui::SharedString::from(id))
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(0x1f3a1f))
        .text_color(rgb(0xc0e0c0))
        .rounded_sm()
        .border_1()
        .border_color(rgb(0x3a6a3a))
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x2f5a2f)).border_color(rgb(0x60a060)))
        .on_mouse_up(MouseButton::Left, on_click)
        .child(div().text_xs().child(glyph))
        .into_any_element()
}
