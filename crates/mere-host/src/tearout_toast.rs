/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Tear-out promotion toast.
//!
//! When the user drops a tile-drag with **no modifier keys**, the
//! default outcome is a leaf tear-out (sticky-note window sharing
//! the donor's session). Per the
//! [tearout-operations brief](../../../design_docs/mere_docs/research/2026-05-11_tearout_operations_brief.md)
//! §3.2 + the [pane-UX brief](../../../design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md)
//! §0, the new window then shows a toast offering to *promote* the
//! leaf to a Branch or Fork — the modifier-keyed shortcuts users
//! who know them (Shift / Ctrl+Shift) bypass the toast; everyone
//! else discovers the trichotomy through it.
//!
//! v0 cut:
//!
//! - Single-state field on `HostRoot.tearout_toast` (`None` =
//!   hidden, `Some` = visible).
//! - Three buttons: **Branch** / **Fork** / **Keep as leaf**.
//! - Bus dispatches `PromoteLeafToBranch` /
//!   `PromoteLeafToFork` (real implementations land in the bus's
//!   execute path); "Keep as leaf" dismisses the toast.
//! - No auto-dismiss timer yet — manual dismiss only. Adding the
//!   timer is a polish slice.

use gpui::{
    Context, IntoElement, MouseButton, MouseUpEvent, Window, div, prelude::*, px, rgb,
};
use mere_host_runtime::{ActionKind, BusAction};

use crate::HostRoot;

/// Live state for an open tear-out promotion toast. Records the
/// donor pane + tile index in case the execute path needs them
/// (e.g. when "Promote to branch" needs the originating workbench
/// to mint its graphlet against).
#[derive(Clone, Copy, Debug)]
pub(crate) struct TearoutToastState {
    pub donor_pane: mere_frame::PaneId,
    pub tile_index: usize,
}

impl HostRoot {
    /// Show the tear-out promotion toast. Replaces any existing
    /// toast (only one visible at a time per window).
    pub(crate) fn show_tearout_toast(
        &mut self,
        state: TearoutToastState,
        cx: &mut Context<Self>,
    ) {
        self.tearout_toast = Some(state);
        cx.notify();
    }

    /// Dismiss the toast. No-op when none is showing.
    pub(crate) fn dismiss_tearout_toast(&mut self, cx: &mut Context<Self>) {
        if self.tearout_toast.take().is_some() {
            cx.notify();
        }
    }

    /// Render the toast overlay if state is present. Positioned
    /// near the top-centre of the workbench area; composed after
    /// the context menu (so the menu, if open, sits on top of
    /// the toast).
    pub(crate) fn render_tearout_toast_overlay(
        &self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let state = self.tearout_toast?;

        let on_branch = cx.listener(
            move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                crate::host_action_bus::dispatch(
                    this,
                    BusAction::pane(state.donor_pane, ActionKind::PromoteLeafToBranch),
                    cx,
                );
                this.dismiss_tearout_toast(cx);
            },
        );
        let on_fork = cx.listener(
            move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                crate::host_action_bus::dispatch(
                    this,
                    BusAction::pane(state.donor_pane, ActionKind::PromoteLeafToFork),
                    cx,
                );
                this.dismiss_tearout_toast(cx);
            },
        );
        let on_keep = cx.listener(
            |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                this.dismiss_tearout_toast(cx);
            },
        );

        Some(
            div()
                .absolute()
                .inset_0()
                // Pointer-events stay enabled only on the toast
                // panel itself — wrapping div is purely positional.
                .flex()
                .flex_col()
                .items_center()
                .pt(px(48.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py(px(8.0))
                        .bg(rgb(0x1c1c1c))
                        .border_1()
                        .border_color(rgb(0x5680c0))
                        .rounded_md()
                        .shadow_lg()
                        .text_sm()
                        .text_color(rgb(0xe0e0e0))
                        .child(
                            div()
                                .text_color(rgb(0xb0b0b0))
                                .mr_2()
                                .child("Leafed out. Promote to\u{2026}"),
                        )
                        .child(toast_button("toast-branch", "Branch", rgb(0x2d4870), on_branch))
                        .child(toast_button("toast-fork", "Fork", rgb(0x2d6048), on_fork))
                        .child(toast_button("toast-keep", "Keep as leaf", rgb(0x404040), on_keep)),
                ),
        )
    }
}

fn toast_button(
    id: &'static str,
    label: &'static str,
    bg: gpui::Rgba,
    on_click: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(gpui::SharedString::from(id))
        .px_3()
        .py(px(4.0))
        .bg(bg)
        .rounded_sm()
        .text_xs()
        .text_color(rgb(0xf0f0f0))
        .cursor(gpui::CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x5a708a)))
        .on_mouse_up(MouseButton::Left, on_click)
        .child(label)
        .into_any_element()
}
