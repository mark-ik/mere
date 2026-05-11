/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Graph switcher dropdown overlay.
//!
//! Lists every graph in the app's registry; clicking a row summons
//! that graph's orrery into the current window. Closed-then-not-yet-
//! reopened graphs (no panel in the layout) show with a "summon"
//! badge; graphs that already have a leaf in this window show with
//! "open here" so the user knows clicking won't do anything new.

use gpui::{
    App, Context, IntoElement, MouseButton, MouseUpEvent, Window, div, prelude::*, px, rgb,
};
use mere_frame::GraphId;

use crate::HostRoot;

impl HostRoot {
    /// Build the graph-switcher dropdown overlay. Lists every
    /// graph in the registry with a click target that summons its
    /// orrery in this window; click-outside closes the dropdown.
    pub(crate) fn render_graph_switcher(
        &self,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let graphs: Vec<(GraphId, String, bool)> = self
            .registry
            .read(cx)
            .iter()
            .map(|(id, _)| {
                let name = self.graph_display_name(id, cx);
                let already_here = self
                    .frame_layout
                    .iter_leaves()
                    .any(|(_, _, gid)| gid == id);
                (id, name, already_here)
            })
            .collect();

        let backdrop_close = cx.listener(
            |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                this.graph_switcher_open = false;
                cx.notify();
            },
        );

        let mut list = div().flex().flex_col().w_full();
        for (graph_id, name, already_here) in graphs {
            let on_select = cx.listener(
                move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                    this.summon_orrery_for_graph(graph_id, cx);
                },
            );
            list = list.child(render_graph_row(graph_id, name, already_here, on_select));
        }

        // Anchor near the top-left (under where the panel buttons
        // live in horizontal-shellbar mode). Click on the dropdown
        // panel itself does NOT close it — only the surrounding
        // backdrop does.
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .pt(px(56.0))
            .pl(px(220.0))
            .bg(gpui::rgba(0x00000060))
            .on_mouse_up(MouseButton::Left, backdrop_close)
            .child(
                div()
                    .w(px(360.0))
                    .max_h(px(420.0))
                    .flex()
                    .flex_col()
                    .bg(rgb(0x1c1c1c))
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded_md()
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_up(MouseButton::Left, |_, _, _| {
                        // Swallow so backdrop doesn't fire when
                        // clicking inside.
                    })
                    .child(
                        div()
                            .px_3()
                            .py_1p5()
                            .text_xs()
                            .text_color(rgb(0x909090))
                            .border_b_1()
                            .border_color(rgb(0x2a2a2a))
                            .child("Graphs in this app"),
                    )
                    .child(
                        div()
                            .id("graph-switcher-list")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_y_scroll()
                            .child(list),
                    ),
            )
    }
}

fn render_graph_row(
    _graph_id: GraphId,
    name: String,
    already_here: bool,
    on_click: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    let (bg, fg) = if already_here {
        (rgb(0x223a55), rgb(0xf0f0f0))
    } else {
        (gpui::rgba(0x00000000), rgb(0xd0d0d0))
    };
    let badge = if already_here { "open here" } else { "summon" };
    let badge_color = if already_here {
        rgb(0xa0c0ff)
    } else {
        rgb(0x80a080)
    };
    div()
        .id("graph-switcher-row")
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_3()
        .py(px(8.0))
        .bg(bg)
        .text_color(fg)
        .border_b_1()
        .border_color(rgb(0x222222))
        .hover(|s| s.bg(rgb(0x2a2a2a)))
        .on_mouse_up(MouseButton::Left, on_click)
        .child(div().flex_1().min_w(px(0.0)).child(name))
        .child(
            div()
                .text_xs()
                .px_2()
                .py(px(2.0))
                .bg(rgb(0x303030))
                .border_1()
                .border_color(rgb(0x444444))
                .rounded_sm()
                .text_color(badge_color)
                .child(badge),
        )
        .into_any_element()
}
