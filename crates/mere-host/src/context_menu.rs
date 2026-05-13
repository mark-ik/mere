/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Right-click context menu component.
//!
//! Per the [pane-UX brief](../../../design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md)
//! §4.2: every surface (node, tile, pane header, switcher row, etc.)
//! can pop a target-scoped context menu on right-click. Entries
//! dispatch through the action bus — same gate, same diagnostics
//! as every other dispatch path. This module owns the
//! **component**; per-surface entry builders live alongside their
//! renderers (orrery_input, panes, graph_switcher).
//!
//! State model: `HostRoot.context_menu: Option<ContextMenuState>`.
//! `Some` = visible at the recorded position with the recorded
//! entries; `None` = hidden. Right-click handlers populate the
//! field; the menu's own click handlers (entries + backdrop) clear
//! it. Reduces to a single field for the whole window — only one
//! menu can be open at a time (web/desktop convention).

use gpui::{
    Context, IntoElement, MouseButton, MouseUpEvent, Pixels, Point, Window, div, prelude::*, px,
    rgb,
};
use mere_host_runtime::BusAction;

use crate::HostRoot;

/// One row in a context menu. Mapping is direct: click → dispatch
/// `action` through the bus.
#[derive(Clone)]
pub(crate) struct ContextMenuEntry {
    pub label: String,
    pub action: BusAction,
    /// Optional keyboard-shortcut hint shown muted on the right
    /// edge of the row. Display-only; the actual keybinding is
    /// registered separately.
    pub shortcut: Option<String>,
    /// When `Some`, the entry is greyed and the string is the
    /// tooltip-style explanation of why. Click is a no-op.
    pub disabled: Option<&'static str>,
    /// When true, a divider is rendered after this entry. Lets
    /// builders group related entries without an extra type.
    pub separator_after: bool,
}

impl ContextMenuEntry {
    /// Convenience: a plain enabled entry with no shortcut hint or
    /// separator.
    pub(crate) fn new(label: impl Into<String>, action: BusAction) -> Self {
        Self {
            label: label.into(),
            action,
            shortcut: None,
            disabled: None,
            separator_after: false,
        }
    }

    pub(crate) fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub(crate) fn with_separator(mut self) -> Self {
        self.separator_after = true;
        self
    }

    #[allow(dead_code)]
    pub(crate) fn disabled(mut self, reason: &'static str) -> Self {
        self.disabled = Some(reason);
        self
    }
}

/// Live state for the open context menu. Anchored at `position`
/// (window-local pixels) and rendering the recorded `entries`.
#[derive(Clone)]
pub(crate) struct ContextMenuState {
    pub position: Point<Pixels>,
    pub entries: Vec<ContextMenuEntry>,
}

impl HostRoot {
    /// Open a context menu at `position` showing `entries`. If a
    /// menu is already open, it's replaced — only one menu is
    /// visible at a time.
    pub(crate) fn open_context_menu(
        &mut self,
        position: Point<Pixels>,
        entries: Vec<ContextMenuEntry>,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = Some(ContextMenuState { position, entries });
        cx.notify();
    }

    /// Close any open context menu. No-op when none is open.
    pub(crate) fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Render the context menu overlay if one is open. Returns a
    /// fully-positioned absolute `div` to be composed onto the
    /// HostRoot frame after every other layer (palette,
    /// graph_switcher) so it's the topmost interactive surface.
    pub(crate) fn render_context_menu_overlay(
        &self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let state = self.context_menu.as_ref()?.clone();

        // Backdrop swallows clicks outside the menu and closes it.
        let backdrop_close = cx.listener(
            |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>| {
                this.close_context_menu(cx);
            },
        );

        let mut list = div().flex().flex_col().w_full();
        for (i, entry) in state.entries.into_iter().enumerate() {
            list = list.child(render_entry(i, entry, cx));
        }

        Some(
            div()
                .absolute()
                .inset_0()
                // Transparent backdrop — captures clicks outside
                // the panel to close, but doesn't darken the
                // window (context menus are lightweight; backdrop
                // should be invisible).
                .on_mouse_up(MouseButton::Left, backdrop_close)
                .child(
                    div()
                        .absolute()
                        .left(state.position.x)
                        .top(state.position.y)
                        .min_w(px(180.0))
                        .max_w(px(320.0))
                        .bg(rgb(0x1c1c1c))
                        .border_1()
                        .border_color(rgb(0x444444))
                        .rounded_md()
                        .shadow_lg()
                        .overflow_hidden()
                        // Swallow mouseup inside the panel so the
                        // backdrop's close handler doesn't fire
                        // when clicking entries (entries handle
                        // their own click + close).
                        .on_mouse_up(MouseButton::Left, |_, _, _| {})
                        .child(list),
                ),
        )
    }
}

fn render_entry(
    index: usize,
    entry: ContextMenuEntry,
    cx: &Context<HostRoot>,
) -> gpui::AnyElement {
    let action = entry.action.clone();
    let disabled = entry.disabled;

    let on_click = cx.listener(
        move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            if disabled.is_some() {
                return;
            }
            // Dispatch first, then close — closes after the action
            // runs so the diagnostic + state mutation reflect what
            // the user did.
            crate::host_action_bus::dispatch(this, action.clone(), cx);
            this.close_context_menu(cx);
        },
    );

    let (label_color, hover_bg) = if disabled.is_some() {
        (rgb(0x707070), rgb(0x1c1c1c))
    } else {
        (rgb(0xe0e0e0), rgb(0x2a2a2a))
    };

    let mut row = div()
        .id(gpui::SharedString::from(format!("ctx-menu-row-{index}")))
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_3()
        .py(px(6.0))
        .text_sm()
        .text_color(label_color)
        .hover(move |s| s.bg(hover_bg))
        .on_mouse_up(MouseButton::Left, on_click)
        .child(div().flex_1().min_w(px(0.0)).child(entry.label));

    if let Some(shortcut) = entry.shortcut {
        row = row.child(
            div()
                .text_xs()
                .text_color(rgb(0x808080))
                .child(shortcut),
        );
    }

    let row_element: gpui::AnyElement = row.into_any_element();

    if entry.separator_after {
        div()
            .flex()
            .flex_col()
            .child(row_element)
            .child(
                div()
                    .h(px(1.0))
                    .bg(rgb(0x303030))
                    .mx_2(),
            )
            .into_any_element()
    } else {
        row_element
    }
}
