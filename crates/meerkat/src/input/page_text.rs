/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Retained document-lane page-text selection and host-side find helpers.

use super::*;
use crate::fetch::{ContentState, Fetched};
use inker::EngineDocument;

impl WindowCtx<'_> {
    pub(crate) fn clear_page_text_selection(&mut self) {
        self.view.page_selection = None;
        self.view.page_text_drag = None;
    }

    pub(crate) fn copy_page_text_selection(&mut self) -> bool {
        let Some(text) = self
            .view
            .page_selection
            .as_ref()
            .filter(|s| self.shared.content.constellation.scene_version(s.member) == s.version)
            .map(|s| s.text.trim().to_string())
            .filter(|s| !s.is_empty())
        else {
            return false;
        };
        if let Some(cb) = self.clipboard.as_mut() {
            let _ = cb.set_text(text);
            return true;
        }
        false
    }

    pub(crate) fn try_begin_page_text_selection(&mut self, x: f32, y: f32) -> bool {
        if self.card_link_at(x, y).is_some() {
            return false;
        }
        let Some((member, version, source_index)) = self.card_source_block_at(x, y) else {
            return false;
        };
        self.update_page_text_selection(member, version, source_index, source_index);
        self.view.page_text_drag = Some(crate::window_view::PageTextDrag {
            member,
            version,
            anchor_source: source_index,
        });
        self.view.request_redraw();
        true
    }

    pub(crate) fn drag_page_text_selection(&mut self, x: f32, y: f32) {
        let Some(drag) = self.view.page_text_drag.as_ref() else {
            return;
        };
        let Some((member, version, source_index)) = self.card_source_block_at(x, y) else {
            return;
        };
        if member != drag.member || version != drag.version {
            return;
        }
        self.update_page_text_selection(member, version, drag.anchor_source, source_index);
        self.view.request_redraw();
    }

    pub(crate) fn finish_page_text_selection(&mut self) -> bool {
        self.view.page_text_drag.take().is_some()
    }

    pub(crate) fn recompute_document_find(
        &mut self,
        member: GraphMemberId,
        query: &str,
    ) -> Option<Vec<Vec<[f32; 4]>>> {
        let packet = self.shared.content.constellation.packet(member)?.0.clone();
        let doc = self.document_lane_doc_for_member(member)?;
        Some(crate::card::find_document_content(&doc, &packet, query))
    }

    fn update_page_text_selection(
        &mut self,
        member: GraphMemberId,
        version: u64,
        anchor_source: usize,
        focus_source: usize,
    ) {
        let Some((packet, _)) = self.shared.content.constellation.packet(member) else {
            self.clear_page_text_selection();
            return;
        };
        let packet = packet.clone();
        let Some(doc) = self.document_lane_doc_for_member(member) else {
            self.clear_page_text_selection();
            return;
        };
        self.view.page_selection =
            crate::card::select_document_content(&doc, &packet, anchor_source, focus_source).map(
                |selection| crate::window_view::PageTextSelection {
                    member,
                    version,
                    rects: selection.rects,
                    text: selection.text,
                },
            );
    }

    fn card_source_block_at(&self, x: f32, y: f32) -> Option<(GraphMemberId, u64, usize)> {
        for (member, rect) in &self.view.content_rects {
            if x < rect[0] || x > rect[2] || y < rect[1] || y > rect[3] {
                continue;
            }
            let scroll = self.view.scroll.get(member).copied().unwrap_or(0.0);
            let version = self.shared.content.constellation.scene_version(*member);
            let Some((packet, _)) = self.shared.content.constellation.packet(*member) else {
                continue;
            };
            let scale = packet.viewport.scale_factor.max(1.0);
            let lx = (x - rect[0]) / scale;
            let ly = ((y - rect[1]) + scroll) / scale;
            let Some(block) = packet.block_at(lx, ly) else {
                continue;
            };
            return Some((*member, version, block.source_block_index));
        }
        None
    }

    fn document_lane_doc_for_member(&mut self, member: GraphMemberId) -> Option<EngineDocument> {
        let url = self
            .orrery()
            .graph()
            .get_node_by_id(member)
            .map(|(_, n)| n.url().to_string())?;
        let state = self.shared.content.pages.get(&url).cloned().or_else(|| {
            self.load_cached(&url).map(|stored| {
                ContentState::Ready(Fetched {
                    content_type: stored.content_type,
                    body: String::from_utf8_lossy(&stored.body).into_owned(),
                })
            })
        });
        crate::card::engine_document_for(
            &url,
            state.as_ref(),
            &self.shared.content.engine_registry,
            &self.shared.content.route_policy,
        )
        .or_else(|| Some(crate::card::content_document(&url, state.as_ref())))
    }
}
