/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # mere-host-gloss
//!
//! gpui renderer for the gloss pane: extracts headings from an
//! [`inker::EngineDocument`] and renders them as an indented outline.
//!
//! Companion to portable [`mere-gloss`] (which projects the same
//! outline into a uxtree subtree for accesskit / automation).

#![doc(html_root_url = "https://docs.rs/mere-host-gloss/0.0.1")]

use gpui::{AnyElement, div, prelude::*, px, rgb};
use inker::{DocumentBlock, EngineDocument, inline_text};

pub fn render(document: &EngineDocument) -> AnyElement {
    let rows: Vec<AnyElement> = document
        .blocks
        .iter()
        .filter_map(|b| match b {
            DocumentBlock::Heading { level, spans } => {
                Some(render_heading_row(*level, &inline_text(spans)))
            }
            _ => None,
        })
        .collect();

    if rows.is_empty() {
        div()
            .text_xs()
            .text_color(rgb(0x707070))
            .child("(no headings)")
            .into_any_element()
    } else {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .children(rows)
            .into_any_element()
    }
}

fn render_heading_row(level: u8, label: &str) -> AnyElement {
    let indent = px((level.saturating_sub(1) as f32) * 12.0);
    let color = match level {
        1 => rgb(0xffffff),
        2 => rgb(0xd0d0d0),
        _ => rgb(0xa0a0a0),
    };
    div()
        .flex()
        .flex_row()
        .gap_2()
        .pl(indent)
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x606060))
                .w(px(16.0))
                .child(format!("h{level}")),
        )
        .child(div().text_color(color).child(label.to_string()))
        .into_any_element()
}
