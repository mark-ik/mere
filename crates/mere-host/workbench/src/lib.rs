/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host-workbench
//!
//! gpui renderer for the workbench pane. The workbench is the surface
//! for examining the **contents of the graph**: every tile is a facet
//! of a graph node, and the strip + body layout lets users keep
//! several open at once without flattening them into a single doc.
//!
//! Per Mark's framing — tiles are not generic UI panes. Other UI
//! concerns (toasts, sidebars, dev tools) belong in their own panes
//! / surfaces, not as tiles.
//!
//! Companion to portable [`mere-workbench`], which owns the workbench's
//! state model + uxtree projection. This crate adds gpui rendering
//! without coupling the portable layer to gpui.

#![doc(html_root_url = "https://docs.rs/mere-host-workbench/0.0.1")]

use gpui::{
    AnyElement, App, CursorStyle, MouseButton, MouseUpEvent, Window, div, prelude::*, px, rgb,
};
use inker::{DocumentBlock, EngineDocument, InlineSpan};

/// One tile in the workbench strip. The host builds these on each
/// render; the renderer pays no attention to identity beyond the
/// index used by `active_index`.
pub struct WorkbenchTile {
    /// Human-readable label — typically the document title or, when
    /// that's empty, the node URL.
    pub label: String,
    /// Click handler: the host selects this tile (swaps it in as
    /// active).
    pub on_select: Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>,
    /// Click handler for the row's close affordance.
    pub on_close: Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>,
}

/// Which side of the workbench pane the tile strip lives on. The
/// host owns the persisted value and passes the current choice in on
/// each render.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StripPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl StripPosition {
    fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

/// Render the workbench pane body — strip on the chosen edge, active
/// document filling the remaining space.
pub fn render(
    tiles: Vec<WorkbenchTile>,
    active_index: Option<usize>,
    active_document: Option<&EngineDocument>,
    strip_position: StripPosition,
) -> AnyElement {
    let strip = render_strip(tiles, active_index, strip_position);
    let body = render_body(active_document);
    let container_row = matches!(
        strip_position,
        StripPosition::Left | StripPosition::Right
    );
    let strip_first = matches!(strip_position, StripPosition::Left | StripPosition::Top);
    let mut container = div().size_full().flex();
    container = if container_row {
        container.flex_row()
    } else {
        container.flex_col()
    };
    if strip_first {
        container.child(strip).child(body)
    } else {
        container.child(body).child(strip)
    }
    .into_any_element()
}

fn render_strip(
    tiles: Vec<WorkbenchTile>,
    active_index: Option<usize>,
    position: StripPosition,
) -> AnyElement {
    let count = tiles.len();
    let vertical = position.is_vertical();
    let mut strip = div()
        .id("workbench-strip")
        .flex()
        .flex_none()
        .bg(rgb(0x171717));
    strip = if vertical {
        strip
            .w(px(180.0))
            .flex_col()
            .overflow_y_scroll()
            .when(position == StripPosition::Left, |s| {
                s.border_r_1().border_color(rgb(0x2a2a2a))
            })
            .when(position == StripPosition::Right, |s| {
                s.border_l_1().border_color(rgb(0x2a2a2a))
            })
    } else {
        strip
            .h(px(30.0))
            .flex_row()
            .overflow_x_scroll()
            .when(position == StripPosition::Top, |s| {
                s.border_b_1().border_color(rgb(0x2a2a2a))
            })
            .when(position == StripPosition::Bottom, |s| {
                s.border_t_1().border_color(rgb(0x2a2a2a))
            })
    };

    for (i, tile) in tiles.into_iter().enumerate() {
        let active = Some(i) == active_index;
        strip = strip.child(render_tile_row(i, tile, active, vertical));
    }

    if count == 0 {
        strip = strip.child(
            div()
                .px_3()
                .py_2()
                .text_xs()
                .text_color(rgb(0x707070))
                .child("no tiles"),
        );
    }

    strip.into_any_element()
}

fn render_tile_row(
    index: usize,
    tile: WorkbenchTile,
    is_active: bool,
    vertical_strip: bool,
) -> AnyElement {
    let row_bg = if is_active {
        rgb(0x223a55)
    } else {
        rgb(0x1c1c1c)
    };
    let label_color = if is_active {
        rgb(0xf0f0f0)
    } else {
        rgb(0xc0c0c0)
    };

    let close_handler = tile.on_close;
    let select_handler = tile.on_select;

    let row = div()
        .id(gpui::SharedString::from(format!("tile-row-{index}")))
        .flex()
        .flex_row()
        .items_center()
        .px_2()
        .py(px(6.0))
        .gap_2()
        .bg(row_bg)
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(rgb(0x2a2a2a)))
        .on_mouse_up(MouseButton::Left, select_handler);
    let row = if vertical_strip {
        // Stripe runs vertically: each tile gets a divider underneath
        // and the label flex-grows to fill the column width.
        row.w_full().border_b_1().border_color(rgb(0x252525))
    } else {
        // Stripe runs horizontally: each tile becomes a tab-like
        // chip, divider on the right, label capped so long titles
        // don't push the strip off-screen.
        row.h_full()
            .max_w(px(180.0))
            .border_r_1()
            .border_color(rgb(0x252525))
    };
    row.child(
        div()
            .flex_1()
            .min_w(px(0.0))
            .text_xs()
            .text_color(label_color)
            .child(tile.label),
    )
    .child(
        div()
            .id(gpui::SharedString::from(format!("tile-close-{index}")))
            .w(px(16.0))
            .h(px(16.0))
            .flex()
            .items_center()
            .justify_center()
            .text_xs()
            .text_color(rgb(0x707070))
            .rounded_sm()
            .hover(|s| s.bg(rgb(0x3a1a1a)).text_color(rgb(0xff8080)))
            .on_mouse_up(MouseButton::Left, close_handler)
            .child("×"),
    )
    .into_any_element()
}

fn render_body(document: Option<&EngineDocument>) -> AnyElement {
    let body = match document {
        Some(doc) => render_document(doc),
        None => div()
            .text_color(rgb(0x707070))
            .child("no tile selected — open one from the orrery or omnibar")
            .into_any_element(),
    };
    div()
        .id("workbench-body")
        .flex_1()
        .min_w(px(0.0))
        .overflow_y_scroll()
        .p_3()
        .child(body)
        .into_any_element()
}

fn render_document(document: &EngineDocument) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .w_full()
        .whitespace_normal()
        .children(document.blocks.iter().map(render_block))
        .into_any_element()
}

fn render_block(block: &DocumentBlock) -> AnyElement {
    match block {
        DocumentBlock::Heading { level, spans } => div()
            .when(*level == 1, |d| d.text_2xl())
            .when(*level == 2, |d| d.text_xl())
            .when(*level >= 3, |d| d.text_lg())
            .text_color(rgb(0xffffff))
            .child(flatten_inline(spans))
            .into_any_element(),

        DocumentBlock::Paragraph { spans } => {
            div().child(flatten_inline(spans)).into_any_element()
        }

        DocumentBlock::CodeBlock { text, .. } => div()
            .bg(rgb(0x2a2a2a))
            .text_color(rgb(0xb0d0ff))
            .p_2()
            .rounded_sm()
            .child(text.clone())
            .into_any_element(),

        DocumentBlock::Quote { blocks } => div()
            .pl_4()
            .border_l_2()
            .border_color(rgb(0x555555))
            .text_color(rgb(0xb0b0b0))
            .flex()
            .flex_col()
            .gap_2()
            .children(blocks.iter().map(render_block))
            .into_any_element(),

        DocumentBlock::List { ordered, items } => div()
            .flex()
            .flex_col()
            .gap_1()
            .children(items.iter().enumerate().map(|(i, item_blocks)| {
                let prefix = if *ordered {
                    format!("{}.", i + 1)
                } else {
                    "•".to_string()
                };
                div()
                    .flex()
                    .gap_2()
                    .child(div().w(px(20.0)).child(prefix))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .gap_1()
                            .children(item_blocks.iter().map(render_block)),
                    )
                    .into_any_element()
            }))
            .into_any_element(),

        DocumentBlock::Image { alt, url } => div()
            .text_color(rgb(0x888888))
            .child(format!("[image: {alt} ({url})]"))
            .into_any_element(),

        DocumentBlock::Preformatted { text } => div()
            .bg(rgb(0x2a2a2a))
            .p_2()
            .rounded_sm()
            .child(text.clone())
            .into_any_element(),

        DocumentBlock::Rule => div().h(px(1.0)).bg(rgb(0x444444)).into_any_element(),

        other => div()
            .bg(rgb(0x331111))
            .text_color(rgb(0xffaaaa))
            .p_2()
            .rounded_sm()
            .child(format!("[unrendered: {other:?}]"))
            .into_any_element(),
    }
}

fn flatten_inline(spans: &[InlineSpan]) -> String {
    inker::inline_text(spans)
}
