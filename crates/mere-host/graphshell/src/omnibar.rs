/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Omnibar widget aspect of the graphshell chrome.
//!
//! Renders the unified entry surface for URL / contextual search /
//! provider search / command arbitration. The text field itself is a
//! live [`OmnibarInput`] entity (owned by the host); this module
//! wraps it in chrome — chip frame, session badge, status text — and
//! does not own any state of its own.
//!
//! See
//! `repos/graphshell/design_docs/graphshell_docs/implementation_strategy/aspect_input/toolbar_omnibar_behavior_spec.md`
//! for the canonical contract the omnibar's session/provider/command
//! logic is growing toward.

use gpui::{AnyElement, Entity, div, prelude::*, px, rgb};
use mere_graphshell::frame_model::{
    OmnibarProviderStatusView, OmnibarSessionKindView, OmnibarViewModel, ToolbarViewModel,
};

use crate::omnibar_input::OmnibarInput;

pub fn render(
    toolbar: &ToolbarViewModel,
    omnibar: Option<&OmnibarViewModel>,
    input: Entity<OmnibarInput>,
) -> AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .flex_1()
        .h(px(28.0))
        .px_3()
        .bg(rgb(0x252525))
        .border_1()
        .border_color(rgb(0x3a3a3a))
        .rounded_sm()
        // The live input grows to fill the chip; chips and status
        // text trail it on the right.
        .child(div().flex_1().min_w(px(0.0)).child(input));

    if let Some(om) = omnibar {
        row = row.child(render_session_chip(om));
    }
    if let Some(status) = &toolbar.status_text {
        row = row.child(
            div()
                .text_xs()
                .text_color(rgb(0x808080))
                .child(status.clone()),
        );
    }
    row.into_any_element()
}

fn render_session_chip(omnibar: &OmnibarViewModel) -> AnyElement {
    let kind = match omnibar.kind {
        OmnibarSessionKindView::Graph => "graph",
        OmnibarSessionKindView::SearchProvider => "search",
    };
    let status = match omnibar.provider_status {
        OmnibarProviderStatusView::Idle => "",
        OmnibarProviderStatusView::Loading => " · loading",
        OmnibarProviderStatusView::Ready => " · ready",
        OmnibarProviderStatusView::FailedNetwork => " · network error",
        OmnibarProviderStatusView::FailedHttp(_) => " · http error",
        OmnibarProviderStatusView::FailedParse => " · parse error",
    };
    div()
        .text_xs()
        .text_color(rgb(0xa0d0ff))
        .child(format!(
            "{kind}: {} match{}{status}",
            omnibar.match_count,
            if omnibar.match_count == 1 { "" } else { "es" }
        ))
        .into_any_element()
}
