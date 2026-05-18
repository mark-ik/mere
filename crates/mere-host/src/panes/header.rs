/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Pane header bar + the tile-row context menu. Split out of
//! `panes.rs` to keep the parent module under the workspace's
//! 600-LOC ceiling.

use gpui::{
    AnyElement, Context, MouseButton, MouseDownEvent, MouseUpEvent, Window, div, prelude::*, px,
    rgb,
};
use mere_frame::{PaneContent, PaneId};

use crate::HostRoot;

/// Tag header for a pane: the content's tag on the left, a `×`
/// close affordance on the right. Clicking × routes to
/// `HostRoot::close_pane` — for orreries that cascades to every
/// panel bound to the same `graph_id`.
pub(super) fn render_pane_header(
    pane_id: PaneId,
    content: &PaneContent,
    cx: &Context<HostRoot>,
) -> AnyElement {
    // Bus-routed close-pane: target = Pane(pane_id); execute path
    // calls HostRoot::close_pane.
    let close = cx.listener(
        move |this, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            crate::host_action_bus::dispatch(
                this,
                mere_host_runtime::BusAction::pane(
                    pane_id,
                    mere_host_runtime::ActionKind::ClosePane,
                ),
                cx,
            );
        },
    );
    // Right-click on the pane header opens the pane context menu.
    // Per the pane-UX brief §4.3: close pane + tear-out modes +
    // reparent (reserved until drag-rearrange lands).
    let content_is_workbench = matches!(content, PaneContent::Workbench);
    let on_right_click = cx.listener(
        move |this, ev: &MouseDownEvent, _window: &mut Window, cx: &mut Context<HostRoot>| {
            use crate::context_menu::ContextMenuEntry;
            use mere_host_runtime::{ActionKind, BusAction, TearOutMode};
            let mut entries = vec![
                ContextMenuEntry::new(
                    "Close pane",
                    BusAction::pane(pane_id, ActionKind::ClosePane),
                )
                .with_separator(),
            ];
            // Tear-out only makes sense for workbenches (the source
            // of tile state). Other pane kinds get a simpler menu.
            if content_is_workbench {
                entries.push(
                    ContextMenuEntry::new(
                        "Tear out as leaf",
                        BusAction::pane(
                            pane_id,
                            ActionKind::TearOutTile {
                                mode: TearOutMode::Leaf,
                                tile_index: None,
                            },
                        ),
                    ),
                );
                entries.push(
                    ContextMenuEntry::new(
                        "Tear out as branch",
                        BusAction::pane(
                            pane_id,
                            ActionKind::TearOutTile {
                                mode: TearOutMode::Branch,
                                tile_index: None,
                            },
                        ),
                    ),
                );
                entries.push(
                    ContextMenuEntry::new(
                        "Tear out as fork",
                        BusAction::pane(
                            pane_id,
                            ActionKind::TearOutTile {
                                mode: TearOutMode::Fork,
                                tile_index: None,
                            },
                        ),
                    )
                    .with_separator(),
                );
            }
            // Drag-to-reparent is wired on the pane header itself
            // (see `render_pane_header`'s `on_drag`). The context
            // menu surfaces the affordance as a hint — disabled
            // because clicking does nothing; the user drags the
            // header to invoke it.
            entries.push(
                ContextMenuEntry::new(
                    "Move pane \u{2014} drag the header",
                    BusAction::app(ActionKind::Quit),
                )
                .disabled("drag the pane header to relocate"),
            );
            this.open_context_menu(ev.position, entries, cx);
        },
    );
    // Make the header a drag source: dragging it carries a
    // `PaneDragPayload { pane_id }`; the drop handler on any pane
    // wrapper resolves the target and fires `ReparentPane`.
    let drag_label = content.tag().to_string();
    let header_id: gpui::ElementId =
        gpui::SharedString::from(format!("pane-header-{}", pane_id.0)).into();
    div()
        .id(header_id)
        .flex()
        .flex_row()
        .items_center()
        .px_3()
        .py_1()
        .text_xs()
        .text_color(rgb(0x808080))
        .bg(rgb(0x141414))
        .border_b_1()
        .border_color(rgb(0x2a2a2a))
        .cursor(gpui::CursorStyle::OpenHand)
        .on_drag(
            mere_host_runtime::PaneDragPayload { pane_id },
            move |_payload, _offset, _window, cx| {
                cx.new(|_| crate::tearout::DraggedPaneLabel {
                    text: drag_label.clone(),
                })
            },
        )
        .on_mouse_down(MouseButton::Right, on_right_click)
        .child(div().flex_1().child(content.tag().to_string()))
        .child(
            div()
                .id(gpui::SharedString::from(format!("pane-close-{}", pane_id.0)))
                .w(px(16.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .text_xs()
                .text_color(rgb(0x707070))
                .rounded_sm()
                .hover(|s| s.bg(rgb(0x3a1a1a)).text_color(rgb(0xff8080)))
                .on_mouse_up(MouseButton::Left, close)
                .child("×"),
        )
        .into_any_element()
}

/// Context menu builder for a tile-strip row. Per the
/// [pane-UX brief](../../../design_docs/mere_docs/design/2026-05-11_pane_ux_design_pass_brief.md)
/// §4.3 tile-strip-tile entries: Focus / Close / Tear-out
/// Leaf/Branch/Fork / Pin to frame (reserved).
pub(super) fn open_tile_row_context_menu(
    this: &mut HostRoot,
    pane_id: PaneId,
    tile_index: usize,
    position: gpui::Point<gpui::Pixels>,
    cx: &mut Context<HostRoot>,
) {
    use crate::context_menu::ContextMenuEntry;
    use mere_host_runtime::{ActionKind, BusAction, TearOutMode};

    // Resolve (node, graph_id) for the "Pin to frame" entry.
    let tile_anchor = this
        .frame_layout
        .iter_leaves()
        .find(|(p, _, _)| *p == pane_id)
        .map(|(_, _, gid)| gid)
        .and_then(|gid| {
            this.panes
                .get(&pane_id)
                .and_then(|s| s.as_workbench())
                .and_then(|w| w.tiles.open_tiles().get(tile_index).copied())
                .map(|node| (node, gid))
        });

    let mut entries = vec![
        ContextMenuEntry::new(
            "Focus tile",
            BusAction::pane(pane_id, ActionKind::FocusTile { index: tile_index }),
        ),
        ContextMenuEntry::new(
            "Close tile",
            BusAction::pane(pane_id, ActionKind::CloseTile { index: tile_index }),
        )
        .with_separator(),
        ContextMenuEntry::new(
            "Tear out as leaf",
            BusAction::pane(
                pane_id,
                ActionKind::TearOutTile {
                    mode: TearOutMode::Leaf,
                    tile_index: Some(tile_index),
                },
            ),
        ),
        ContextMenuEntry::new(
            "Tear out as branch",
            BusAction::pane(
                pane_id,
                ActionKind::TearOutTile {
                    mode: TearOutMode::Branch,
                    tile_index: Some(tile_index),
                },
            ),
        ),
        ContextMenuEntry::new(
            "Tear out as fork",
            BusAction::pane(
                pane_id,
                ActionKind::TearOutTile {
                    mode: TearOutMode::Fork,
                    tile_index: Some(tile_index),
                },
            ),
        )
        .with_separator(),
    ];
    if let Some((node, graph_id)) = tile_anchor {
        entries.push(ContextMenuEntry::new(
            "Pin to frame",
            BusAction::pane(
                pane_id,
                ActionKind::PinTileToFrame {
                    node: mere_frame::LeafNodeRef(node.index() as u32),
                    graph_id,
                },
            ),
        ));
    } else {
        entries.push(
            ContextMenuEntry::new("Pin to frame", BusAction::app(ActionKind::Quit))
                .disabled("could not resolve tile node"),
        );
    }
    this.open_context_menu(position, entries, cx);
}
