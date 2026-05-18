/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pinned-tile leaf renderer. Split out of `panes.rs` to keep the
//! parent module under the workspace's 600-LOC ceiling.

use gpui::{AnyElement, div, prelude::*, rgb};
use mere_frame::{GraphId, PaneContent};

use super::PaneRenderCtx;

/// Render a **pinned tile** leaf — a single tile's document
/// surface without the workbench strip. Per the pane-UX brief §3
/// frametree side-by-side rendering: lets users keep a reference
/// tile visible while navigating in a sibling workbench.
///
/// Document lookup v0: walks every workbench pane in this window;
/// uses the first one whose TileState has the node cached. When
/// no workbench has it (closed all relevant tiles), renders a
/// placeholder. A later slice could re-fetch via `inker::loader`
/// to make pinned tiles self-sufficient.
pub(super) fn render_pinned_tile_pane(
    leaf_node_ref: mere_frame::LeafNodeRef,
    graph_id: GraphId,
    pctx: &PaneRenderCtx,
) -> AnyElement {
    let node = mere_kernel::graph::NodeKey::new(leaf_node_ref.0 as usize);
    // Walk every workbench in the window with matching graph_id;
    // first hit's TileState.documents wins.
    let mut found_doc = None;
    for (other_pid, other_content, other_gid) in pctx.frame_layout.iter_leaves() {
        if !matches!(other_content, PaneContent::Workbench) || other_gid != graph_id {
            continue;
        }
        if let Some(workbench) = pctx.panes.get(&other_pid).and_then(|s| s.as_workbench()) {
            if let Some(doc) = workbench.tiles.document_for(node) {
                found_doc = Some(doc);
                break;
            }
        }
    }
    match found_doc {
        Some(doc) => {
            // Render just the document body — no strip. Reuse the
            // gloss renderer which already takes &EngineDocument
            // and produces a clean body view.
            mere_host_gloss::render(doc)
        }
        None => div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .text_color(rgb(0x707070))
            .text_sm()
            .child("Pinned tile not loaded")
            .child(
                div()
                    .text_xs()
                    .mt_2()
                    .child("open the tile in a workbench to populate"),
            )
            .into_any_element(),
    }
}
