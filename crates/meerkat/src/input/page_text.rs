/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Retained document-lane page-text selection and host-side find helpers.

use super::*;
use crate::fetch::{ContentState, Fetched};
use inker::EngineDocument;

struct PageTextHit {
    member: GraphMemberId,
    gens: armillary::Generations,
    local: (f32, f32),
    in_tile: bool,
    source_index: Option<usize>,
}

impl WindowCtx<'_> {
    pub(crate) fn clear_page_text_selection(&mut self) {
        if let Some(member) = self.page_text_cache_member() {
            self.shared
                .content
                .constellation
                .clear_page_text_selection(member);
        }
        self.view.page_selection = None;
        self.view.page_text_drag = None;
    }

    pub(crate) fn copy_page_text_selection(&mut self) -> bool {
        let Some(text) = self
            .view
            .page_selection
            .as_ref()
            .filter(|s| self.shared.content.constellation.generations(s.member) == Some(s.gens))
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
        if self.tile_link_at(x, y).is_some() || self.card_link_at(x, y).is_some() {
            return false;
        }
        let Some(hit) = self.page_text_hit_at(x, y) else {
            return false;
        };
        self.shared
            .content
            .constellation
            .clear_page_text_selection(hit.member);
        self.view.page_selection = None;
        if hit.in_tile {
            self.focus_workbench_member(hit.member);
        }
        let anchor = if let Some(source_index) = hit.source_index {
            self.update_document_page_text_selection(
                hit.member,
                hit.gens,
                source_index,
                source_index,
            );
            crate::window_view::PageTextAnchor::Document { source_index }
        } else {
            self.request_html_page_text_selection(hit.member, hit.local, hit.local);
            crate::window_view::PageTextAnchor::Html { point: hit.local }
        };
        self.view.page_text_drag = Some(crate::window_view::PageTextDrag {
            member: hit.member,
            gens: hit.gens,
            anchor,
        });
        self.view.request_redraw();
        true
    }

    pub(crate) fn refresh_page_text_selection(&mut self) {
        if self.view.page_selection.as_ref().is_some_and(|selection| {
            self.shared
                .content
                .constellation
                .generations(selection.member)
                != Some(selection.gens)
        }) {
            self.view.page_selection = None;
        }
        let dragging_html = self.view.page_text_drag.as_ref().is_some_and(|drag| {
            matches!(drag.anchor, crate::window_view::PageTextAnchor::Html { .. })
        });
        let member = self.page_text_cache_member();
        let Some(member) = member else {
            return;
        };
        let Some((selection, gens)) = self
            .shared
            .content
            .constellation
            .page_text_selection(member)
        else {
            return;
        };
        if dragging_html
            || self.view.page_selection.is_none()
            || self
                .view
                .page_selection
                .as_ref()
                .is_some_and(|s| s.member == member)
        {
            self.view.page_selection = Some(crate::window_view::PageTextSelection {
                member,
                gens,
                rects: selection.rects.clone(),
                text: selection.text.clone(),
            });
        }
    }

    pub(crate) fn drag_page_text_selection(&mut self, x: f32, y: f32) {
        let Some(drag) = self.view.page_text_drag.as_ref() else {
            return;
        };
        let member = drag.member;
        let gens = drag.gens;
        let anchor = match &drag.anchor {
            crate::window_view::PageTextAnchor::Document { source_index } => {
                crate::window_view::PageTextAnchor::Document {
                    source_index: *source_index,
                }
            }
            crate::window_view::PageTextAnchor::Html { point } => {
                crate::window_view::PageTextAnchor::Html { point: *point }
            }
        };
        let Some(hit) = self.page_text_hit_at(x, y) else {
            return;
        };
        if hit.member != member || hit.gens != gens {
            return;
        }
        match anchor {
            crate::window_view::PageTextAnchor::Document { source_index } => {
                let Some(focus_source) = hit.source_index else {
                    return;
                };
                self.update_document_page_text_selection(member, gens, source_index, focus_source);
            }
            crate::window_view::PageTextAnchor::Html { point } => {
                self.request_html_page_text_selection(member, point, hit.local);
            }
        }
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

    fn update_document_page_text_selection(
        &mut self,
        member: GraphMemberId,
        gens: armillary::Generations,
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
                    gens,
                    rects: selection.rects,
                    text: selection.text,
                },
            );
    }

    fn request_html_page_text_selection(
        &mut self,
        member: GraphMemberId,
        anchor: (f32, f32),
        focus: (f32, f32),
    ) {
        self.shared
            .content
            .constellation
            .request_text_selection(member, anchor, focus);
    }

    fn page_text_cache_member(&self) -> Option<GraphMemberId> {
        self.view
            .page_text_drag
            .as_ref()
            .map(|drag| drag.member)
            .or_else(|| {
                self.view
                    .page_selection
                    .as_ref()
                    .map(|selection| selection.member)
            })
            .or(self.view.focused_tile)
            .or_else(|| self.focused_member())
    }

    fn page_text_hit_at(&self, x: f32, y: f32) -> Option<PageTextHit> {
        self.page_text_hit_in(&self.view.tile_rects, x, y, true)
            .or_else(|| self.page_text_hit_in(&self.view.content_rects, x, y, false))
    }

    fn page_text_hit_in(
        &self,
        rects: &[(GraphMemberId, [f32; 4])],
        x: f32,
        y: f32,
        in_tile: bool,
    ) -> Option<PageTextHit> {
        for (member, rect) in rects {
            if x < rect[0] || x > rect[2] || y < rect[1] || y > rect[3] {
                continue;
            }
            let Some(gens) = self.shared.content.constellation.generations(*member) else {
                continue;
            };
            let scroll = self.view.scroll.get(member).copied().unwrap_or(0.0);
            let local = (x - rect[0], (y - rect[1]) + scroll);
            let source_index =
                self.shared
                    .content
                    .constellation
                    .packet(*member)
                    .and_then(|(packet, _)| {
                        let scale = packet.viewport.scale_factor.max(1.0);
                        packet
                            .block_at(local.0 / scale, local.1 / scale)
                            .map(|block| block.source_block_index)
                    });
            return Some(PageTextHit {
                member: *member,
                gens,
                local,
                in_tile,
                source_index,
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestShell;
    use document_canvas::{DocumentRenderPacket, RenderedBlock, RenderedBlockKind};
    use winit::event::{ElementState, MouseButton};
    use winit::event_loop::EventLoopProxy;

    fn test_app() -> TestShell {
        let (_tx, rx) = std::sync::mpsc::channel();
        let temp = crate::test_support::temp_session_dir("mere-page-text-tests");
        let shell = crate::Shell::new_with_session_dir(test_proxy(), rx, temp.path().to_path_buf());
        TestShell::new(shell, temp)
    }

    fn test_proxy() -> EventLoopProxy<()> {
        crate::test_support::event_loop_proxy()
    }

    fn wait_for_packet(
        app: &mut TestShell,
        member: GraphMemberId,
        url: &str,
        width: u32,
        height: u32,
    ) -> DocumentRenderPacket {
        let graph = app.view().focused_graph;
        app.shared
            .content
            .constellation
            .reconcile(&[(member, graph)]);
        let sheet = app.shared.presentation.document_sheet_composed();
        app.shared
            .content
            .constellation
            .drive(member, url, None, width, height, sheet, "");
        for _ in 0..80 {
            app.shared.content.constellation.drain();
            if let Some((packet, _)) = app.shared.content.constellation.packet(member) {
                return packet.clone();
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("document packet never arrived for {url}");
    }

    fn collect_leaf_blocks<'a>(blocks: &'a [RenderedBlock], out: &mut Vec<&'a RenderedBlock>) {
        for block in blocks {
            if let RenderedBlockKind::Group { children } = &block.kind {
                collect_leaf_blocks(children, out);
            } else {
                out.push(block);
            }
        }
    }

    fn block_window_point(
        packet: &DocumentRenderPacket,
        dest: [f32; 4],
        block: &RenderedBlock,
    ) -> (f32, f32) {
        let scale = packet.viewport.scale_factor.max(1.0);
        let x = dest[0] + (block.bounds.origin.x + block.bounds.max_x()) * 0.5 * scale;
        let y = dest[1] + (block.bounds.origin.y + block.bounds.max_y()) * 0.5 * scale;
        (x, y)
    }

    #[test]
    fn workbench_press_arms_retained_page_text_selection() {
        let mut app = test_app();
        let member = app
            .orrery()
            .focused_member()
            .expect("welcome is focused at startup");
        let url = app
            .orrery()
            .focused_url()
            .expect("focused welcome url")
            .to_string();
        {
            let mut wc = app.ctx();
            wc.open_workbench();
            wc.view.workbench.open_tile(member);
            wc.view.focused_tile = Some(member);
            wc.view.active_content = crate::ContentPane::Workbench;
        }
        let packet = wait_for_packet(&mut app, member, &url, 360, 240);
        let mut leaves = Vec::new();
        collect_leaf_blocks(&packet.blocks, &mut leaves);
        assert!(
            leaves.len() >= 2,
            "welcome packet should expose multiple leaf blocks"
        );
        let dest = [120.0, 140.0, 480.0, 380.0];
        let start = block_window_point(&packet, dest, leaves[0]);
        let end = block_window_point(&packet, dest, leaves[1]);
        {
            let mut wc = app.ctx();
            wc.view.content_rects = vec![(member, dest)];
            wc.view.tile_rects = vec![(member, dest)];
            wc.view.cursor = start;
            wc.on_mouse_input(ElementState::Pressed, MouseButton::Left);
            assert!(
                wc.view.page_text_drag.is_some(),
                "a workbench press over retained text should arm drag selection"
            );
            assert!(
                !wc.view.workbench_gesture,
                "retained selection press should not hand the gesture to pelt"
            );
            assert!(
                wc.view.page_selection.is_some(),
                "the anchor block should select immediately on press"
            );
            wc.drag_page_text_selection(end.0, end.1);
            let selection = wc
                .view
                .page_selection
                .as_ref()
                .expect("drag keeps a live page selection");
            assert!(
                selection.text.contains("Mere"),
                "selection should include the heading block"
            );
            assert!(
                selection.text.contains("A graph-shaped browser"),
                "selection should extend into the next paragraph block"
            );
            wc.on_mouse_input(ElementState::Released, MouseButton::Left);
            assert!(
                wc.view.page_text_drag.is_none(),
                "release ends the workbench page-text drag"
            );
            assert!(
                wc.view.page_selection.is_some(),
                "release keeps the selected text available for copy"
            );
        }
    }
}
