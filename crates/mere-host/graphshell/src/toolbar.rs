/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Toolbar aspect — composes the chrome row from nav + omnibar +
//! status. The crate's public [`render`] function lives here.
//!
//! When the shellbar is on the **top or bottom** edge, the chrome
//! lays out as a horizontal row: nav buttons on the left, omnibar
//! filling the rest. When it's on the **left or right**, the chrome
//! becomes a 280px-wide sidebar — nav buttons in a small horizontal
//! row at the top, omnibar below as a horizontal text field that
//! fills the column width.

use gpui::{AnyElement, App, Entity, MouseUpEvent, Window, div, prelude::*, px, rgb};
use mere_graphshell::frame_model::{OmnibarViewModel, ToolbarViewModel};

use crate::omnibar_input::OmnibarInput;
use crate::panel_buttons::PanelButtonState;
use crate::{nav, omnibar, panel_buttons};

/// Orientation in which to lay out the shellbar's internals.
///
/// `Horizontal` is the conventional browser-style top/bottom strip.
/// `Vertical` is the sidebar form used when the shellbar lives on
/// the window's left or right edge.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShellbarOrientation {
    Horizontal,
    Vertical,
}

/// Fixed sidebar width when the shellbar runs along a window edge.
pub const VERTICAL_SHELLBAR_WIDTH: f32 = 280.0;

#[allow(clippy::too_many_arguments)]
pub fn render(
    toolbar: &ToolbarViewModel,
    omnibar_session: Option<&OmnibarViewModel>,
    input: Entity<OmnibarInput>,
    orientation: ShellbarOrientation,
    panel_state: PanelButtonState,
    graph_switcher_open: bool,
    on_back: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_forward: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_reload: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_workbench: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_gloss: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_apparatus: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_summon_orrery_new_graph: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    on_toggle_graph_switcher: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let nav_row = nav::render(toolbar, on_back, on_forward, on_reload);
    let panel_row = panel_buttons::render(
        panel_state,
        on_toggle_workbench,
        on_toggle_gloss,
        on_toggle_apparatus,
        on_summon_orrery_new_graph,
        on_toggle_graph_switcher,
        graph_switcher_open,
    );
    let omnibar_row = omnibar::render(toolbar, omnibar_session, input);
    match orientation {
        ShellbarOrientation::Horizontal => div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .py_2()
            .bg(rgb(0x141414))
            .border_b_1()
            .border_color(rgb(0x2a2a2a))
            .child(nav_row)
            .child(panel_row)
            .child(omnibar_row)
            .into_any_element(),
        ShellbarOrientation::Vertical => div()
            .flex()
            .flex_col()
            .items_stretch()
            .gap_3()
            .p_3()
            .w(px(VERTICAL_SHELLBAR_WIDTH))
            .flex_none()
            .bg(rgb(0x141414))
            .border_color(rgb(0x2a2a2a))
            .child(nav_row)
            .child(panel_row)
            .child(omnibar_row)
            .into_any_element(),
    }
}
